use std::{ffi::c_void, ptr};
use windows::core::GUID;
pub const INTERFACE_GUID: GUID = GUID::from_u128(0xe738e953_85a0_4b6d_b203_19fa8e95b1d4);
pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}
pub fn winerr() -> String {
    std::io::Error::last_os_error().to_string()
}
#[repr(C)]
pub struct DeviceInterface {
    pub size: u32,
    pub class: GUID,
    pub flags: u32,
    pub reserved: usize,
}
#[repr(C)]
#[derive(Default)]
pub struct Device {
    pub size: u32,
    pub class: GUID,
    pub instance: u32,
    pub reserved: usize,
}
#[repr(C, packed)]
#[derive(Default)]
struct Descriptor {
    length: u8,
    kind: u8,
    number: u8,
    alternate: u8,
    endpoints: u8,
    class: u8,
    subclass: u8,
    protocol: u8,
    index: u8,
}
#[repr(C)]
#[derive(Default)]
struct Pipe {
    kind: i32,
    id: u8,
    packet: u16,
    interval: u8,
}
#[link(name = "setupapi")]
unsafe extern "system" {
    pub fn SetupDiGetClassDevsW(
        class: *const GUID,
        enumerator: *const u16,
        window: isize,
        flags: u32,
    ) -> isize;
    pub fn SetupDiEnumDeviceInterfaces(
        set: isize,
        device: *const Device,
        class: *const GUID,
        index: u32,
        info: *mut DeviceInterface,
    ) -> i32;
    pub fn SetupDiGetDeviceInterfaceDetailW(
        set: isize,
        info: *mut DeviceInterface,
        detail: *mut u8,
        size: u32,
        required: *mut u32,
        device: *mut Device,
    ) -> i32;
    pub fn SetupDiDestroyDeviceInfoList(set: isize) -> i32;
}
#[link(name = "kernel32")]
unsafe extern "system" {
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        creation: u32,
        flags: u32,
        template: isize,
    ) -> isize;
    fn CloseHandle(handle: isize) -> i32;
    fn GetLastError() -> u32;
}
#[link(name = "winusb")]
unsafe extern "system" {
    fn WinUsb_Initialize(file: isize, usb: *mut isize) -> i32;
    fn WinUsb_Free(usb: isize) -> i32;
    fn WinUsb_QueryInterfaceSettings(usb: isize, alternate: u8, descriptor: *mut Descriptor)
        -> i32;
    fn WinUsb_QueryPipe(usb: isize, alternate: u8, index: u8, pipe: *mut Pipe) -> i32;
    fn WinUsb_SetPipePolicy(usb: isize, pipe: u8, kind: u32, len: u32, value: *const u32) -> i32;
    fn WinUsb_ReadPipe(
        usb: isize,
        pipe: u8,
        buffer: *mut u8,
        len: u32,
        read: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
}
pub struct Usb {
    file: isize,
    usb: isize,
    pipe: u8,
}
impl Usb {
    pub fn open() -> Result<Self, String> {
        let path =
            find_path()?.ok_or("未找到独立映射接口；请插入接收器，首次使用需启用独立按键映射")?;
        let file = unsafe {
            CreateFileW(
                wide(&path).as_ptr(),
                0xc0000000,
                0,
                ptr::null(),
                3,
                0x40000000,
                0,
            )
        };
        if file == -1 {
            return Err(format!(
                "接收器被占用或无法打开，请退出独立海信适配程序：{}",
                winerr()
            ));
        }
        let mut dev = Self {
            file,
            usb: 0,
            pipe: 0,
        };
        if unsafe { WinUsb_Initialize(file, &mut dev.usb) } == 0 {
            return Err(winerr());
        }
        let mut descriptor = Descriptor::default();
        if unsafe { WinUsb_QueryInterfaceSettings(dev.usb, 0, &mut descriptor) } == 0
            || descriptor.number != 2
        {
            return Err("USB 接口身份不匹配".into());
        }
        for i in 0..descriptor.endpoints {
            let mut pipe = Pipe::default();
            if unsafe { WinUsb_QueryPipe(dev.usb, 0, i, &mut pipe) } != 0
                && pipe.kind == 3
                && pipe.id & 0x80 != 0
            {
                dev.pipe = pipe.id;
                break;
            }
        }
        if dev.pipe == 0 {
            return Err("接收器没有中断输入端点".into());
        }
        let timeout = 12u32;
        if unsafe { WinUsb_SetPipePolicy(dev.usb, dev.pipe, 3, 4, &timeout) } == 0 {
            return Err(winerr());
        }
        Ok(dev)
    }
    pub fn read(&self, buf: &mut [u8]) -> Result<usize, String> {
        let mut read = 0;
        if unsafe {
            WinUsb_ReadPipe(
                self.usb,
                self.pipe,
                buf.as_mut_ptr(),
                buf.len() as u32,
                &mut read,
                ptr::null_mut(),
            )
        } != 0
        {
            return Ok(read as usize);
        }
        if unsafe { GetLastError() } == 121 {
            return Ok(0);
        }
        Err(winerr())
    }
}
impl Drop for Usb {
    fn drop(&mut self) {
        unsafe {
            if self.usb != 0 {
                WinUsb_Free(self.usb);
            }
            if self.file != -1 {
                CloseHandle(self.file);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Explicit device check only: never injects keys, moves the cursor or records audio.
    #[test]
    #[ignore = "requires the connected Hisense receiver and exclusive access"]
    fn connected_receiver_read_only() {
        use cpal::traits::{DeviceTrait, HostTrait};
        let usb = Usb::open().expect("connect the receiver and stop other Hisense adapters");
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut counts = std::collections::BTreeMap::<u8, usize>::new();
        let mut bytes = [0; 512];
        while std::time::Instant::now() < deadline {
            let n = usb.read(&mut bytes).expect("receiver read must succeed");
            if n > 0 {
                *counts.entry(bytes[0]).or_default() += 1;
            }
        }
        eprintln!("Hisense interface 02 opened; 2-second report counts: {counts:?}");
        let microphones: Vec<_> = cpal::default_host()
            .input_devices()
            .unwrap()
            .filter_map(|d| d.name().ok())
            .filter(|n| {
                let n = n.to_lowercase();
                n.contains("audio-hid") || n.contains("hisense") || n.contains("drf3u65")
            })
            .collect();
        eprintln!("Hisense microphone candidates (enumeration only): {microphones:?}");
    }
}
fn find_path() -> Result<Option<String>, String> {
    let set = unsafe { SetupDiGetClassDevsW(&INTERFACE_GUID, ptr::null(), 0, 0x12) };
    if set == -1 {
        return Err(winerr());
    }
    let result = (|| {
        for index in 0..32 {
            let mut info = DeviceInterface {
                size: std::mem::size_of::<DeviceInterface>() as u32,
                class: GUID::zeroed(),
                flags: 0,
                reserved: 0,
            };
            if unsafe {
                SetupDiEnumDeviceInterfaces(set, ptr::null(), &INTERFACE_GUID, index, &mut info)
            } == 0
            {
                break;
            }
            let mut size = 0;
            unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    set,
                    &mut info,
                    ptr::null_mut(),
                    0,
                    &mut size,
                    ptr::null_mut(),
                );
            }
            if !(8..=32768).contains(&size) {
                continue;
            }
            let mut detail = vec![0u8; size as usize];
            detail[..4].copy_from_slice(&8u32.to_le_bytes());
            if unsafe {
                SetupDiGetDeviceInterfaceDetailW(
                    set,
                    &mut info,
                    detail.as_mut_ptr(),
                    size,
                    &mut size,
                    ptr::null_mut(),
                )
            } == 0
            {
                continue;
            }
            let chars: Vec<u16> = detail[4..]
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .take_while(|ch| *ch != 0)
                .collect();
            let path = String::from_utf16_lossy(&chars);
            if path
                .to_ascii_lowercase()
                .contains("vid_109b&pid_3834&mi_02")
            {
                return Ok(Some(path));
            }
        }
        Ok(None)
    })();
    unsafe {
        SetupDiDestroyDeviceInfoList(set);
    }
    result
}
