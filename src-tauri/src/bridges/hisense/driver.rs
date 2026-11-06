use super::usb::{wide, winerr, Device, SetupDiDestroyDeviceInfoList, SetupDiGetClassDevsW};
use std::{path::PathBuf, ptr};
#[repr(C)]
struct Driver {
    size: u32,
    kind: u32,
    reserved: usize,
    description: [u16; 256],
    manufacturer: [u16; 256],
    provider: [u16; 256],
    date: [u32; 2],
    version: u64,
}
#[repr(C)]
struct Parameters {
    size: u32,
    flags: u32,
    flags_ex: u32,
    parent: usize,
    handler: usize,
    context: usize,
    queue: usize,
    class_reserved: usize,
    reserved: u32,
    path: [u16; 260],
}
#[link(name = "setupapi")]
unsafe extern "system" {
    fn SetupDiEnumDeviceInfo(set: isize, index: u32, device: *mut Device) -> i32;
    fn SetupDiGetDeviceInstanceIdW(
        set: isize,
        device: *mut Device,
        text: *mut u16,
        size: u32,
        required: *mut u32,
    ) -> i32;
    fn SetupDiGetDeviceRegistryPropertyW(
        set: isize,
        device: *mut Device,
        property: u32,
        kind: *mut u32,
        buffer: *mut u8,
        size: u32,
        required: *mut u32,
    ) -> i32;
    fn SetupDiSetDeviceRegistryPropertyW(
        set: isize,
        device: *mut Device,
        property: u32,
        value: *const u8,
        size: u32,
    ) -> i32;
    fn SetupDiGetDeviceInstallParamsW(
        set: isize,
        device: *mut Device,
        params: *mut Parameters,
    ) -> i32;
    fn SetupDiSetDeviceInstallParamsW(
        set: isize,
        device: *mut Device,
        params: *mut Parameters,
    ) -> i32;
    fn SetupDiBuildDriverInfoList(set: isize, device: *mut Device, kind: u32) -> i32;
    fn SetupDiEnumDriverInfoW(
        set: isize,
        device: *mut Device,
        kind: u32,
        index: u32,
        driver: *mut Driver,
    ) -> i32;
    fn SetupDiGetDriverInfoDetailW(
        set: isize,
        device: *mut Device,
        driver: *mut Driver,
        detail: *mut u8,
        size: u32,
        required: *mut u32,
    ) -> i32;
    fn SetupDiDestroyDriverInfoList(set: isize, device: *mut Device, kind: u32) -> i32;
    fn SetupDiCreateDevRegKeyW(
        set: isize,
        device: *mut Device,
        scope: u32,
        profile: u32,
        kind: u32,
        inf: isize,
        section: *const u16,
    ) -> isize;
}
#[link(name = "advapi32")]
unsafe extern "system" {
    fn RegSetValueExW(
        key: isize,
        name: *const u16,
        reserved: u32,
        kind: u32,
        value: *const u8,
        size: u32,
    ) -> i32;
    fn RegCloseKey(key: isize) -> i32;
}
#[link(name = "newdev")]
unsafe extern "system" {
    fn DiInstallDevice(
        window: usize,
        set: isize,
        device: *mut Device,
        driver: *mut Driver,
        flags: u32,
        reboot: *mut i32,
    ) -> i32;
}
fn check(ok: i32) -> Result<(), String> {
    if ok != 0 {
        Ok(())
    } else {
        Err(winerr())
    }
}
fn read_wide(bytes: &[u8]) -> String {
    String::from_utf16_lossy(
        &bytes
            .chunks_exact(2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .take_while(|x| *x != 0)
            .collect::<Vec<_>>(),
    )
}
fn result_path() -> PathBuf {
    PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
        .join("NexusPrimeHisense")
        .join("driver-result.json")
}
pub fn run_cli(args: &[String]) -> i32 {
    let enabled = match args.get(2).map(String::as_str) {
        Some("setup") => true,
        Some("restore") => false,
        _ => return 2,
    };
    let result = install(enabled);
    let payload = match &result {
        Ok(reboot) => serde_json::json!({"ok":true,"reboot":reboot}),
        Err(error) => serde_json::json!({"ok":false,"error":error}),
    };
    let path = result_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, payload.to_string());
    if result.is_ok() {
        0
    } else {
        1
    }
}
pub fn request(enabled: bool) -> Result<String, String> {
    let _ = std::fs::remove_file(result_path());
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let escaped = exe.to_string_lossy().replace('\'', "''");
    let action = if enabled { "setup" } else { "restore" };
    let script=format!("$ErrorActionPreference='Stop'; $p=Start-Process -FilePath '{escaped}' -ArgumentList 'hisense-driver','{action}' -Verb RunAs -WindowStyle Hidden -PassThru -Wait; exit $p.ExitCode");
    let status = crate::windows_command::background_command("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() && !result_path().exists() {
        return Err("接收器配置未完成（管理员确认可能被取消）。原有电脑键盘不会被更改。".into());
    }
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(result_path()).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
    if value["ok"] != true {
        return Err(value["error"].as_str().unwrap_or("接收器配置失败").into());
    }
    Ok(if value["reboot"] == true {
        "配置已保存，需要重启 Windows 后使用"
    } else if enabled {
        "已启用海信接收器独立映射"
    } else {
        "已恢复海信接收器原始 HID 驱动"
    }
    .into())
}
pub fn install(enabled: bool) -> Result<bool, String> {
    if std::mem::size_of::<Device>() != 32
        || std::mem::size_of::<Driver>() != 1568
        || std::mem::size_of::<Parameters>() != 584
    {
        return Err("仅支持 64 位 Windows".into());
    }
    let set = unsafe { SetupDiGetClassDevsW(ptr::null(), wide("USB").as_ptr(), 0, 6) };
    if set == -1 {
        return Err(winerr());
    }
    let result = (|| {
        let mut targets = Vec::new();
        for index in 0..1024 {
            let mut d: Device = unsafe { std::mem::zeroed() };
            d.size = 32;
            if unsafe { SetupDiEnumDeviceInfo(set, index, &mut d) } == 0 {
                break;
            }
            let mut text = [0u16; 1024];
            if unsafe {
                SetupDiGetDeviceInstanceIdW(set, &mut d, text.as_mut_ptr(), 1024, ptr::null_mut())
            } != 0
            {
                let id = String::from_utf16_lossy(
                    &text.into_iter().take_while(|x| *x != 0).collect::<Vec<_>>(),
                );
                if id
                    .to_ascii_uppercase()
                    .starts_with("USB\\VID_109B&PID_3834&MI_02\\")
                {
                    targets.push((d, id));
                }
            }
        }
        if targets.len() != 1 {
            return Err("请连接且只连接一个海信 DRF3U65 接收器".into());
        }
        let (mut d, id) = targets.remove(0);
        let mut service = [0u8; 1024];
        let mut required = 0;
        check(unsafe {
            SetupDiGetDeviceRegistryPropertyW(
                set,
                &mut d,
                4,
                ptr::null_mut(),
                service.as_mut_ptr(),
                1024,
                &mut required,
            )
        })?;
        let service = read_wide(&service[..required as usize]);
        if !matches!(service.to_ascii_lowercase().as_str(), "hidusb" | "winusb") {
            return Err("接收器原驱动不符合预期，已停止修改".into());
        }
        let backup = result_path().with_file_name("driver-before-change.json");
        if let Some(parent) = backup.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        std::fs::write(
            backup,
            serde_json::json!({"instance":id,"service":service}).to_string(),
        )
        .map_err(|e| e.to_string())?;
        if enabled {
            let key = unsafe { SetupDiCreateDevRegKeyW(set, &mut d, 1, 0, 1, 0, ptr::null()) };
            if key == -1 {
                return Err(winerr());
            }
            let mut value = wide("{E738E953-85A0-4B6D-B203-19FA8E95B1D4}");
            value.push(0);
            let error = unsafe {
                RegSetValueExW(
                    key,
                    wide("DeviceInterfaceGUIDs").as_ptr(),
                    0,
                    7,
                    value.as_ptr() as *const u8,
                    (value.len() * 2) as u32,
                )
            };
            unsafe {
                RegCloseKey(key);
            }
            if error != 0 {
                return Err(format!("接收器接口注册失败：{error}"));
            }
        }
        let result = bind(set, &mut d, enabled);
        if let Err(error) = &result {
            if let Err(rollback) = bind(set, &mut d, service.eq_ignore_ascii_case("winusb")) {
                return Err(format!(
                    "{error}；恢复先前驱动也失败：{rollback}。请重新插拔接收器后使用恢复原驱动。"
                ));
            }
        }
        result
    })();
    unsafe {
        SetupDiDestroyDeviceInfoList(set);
    }
    result
}
fn bind(set: isize, d: &mut Device, enabled: bool) -> Result<bool, String> {
    let class = wide(if enabled {
        "{88BAE032-5A81-49F0-BC3D-A4FF138216D6}"
    } else {
        "{745A17A0-74D3-11D0-B6FE-00A0C90F57DA}"
    });
    check(unsafe {
        SetupDiSetDeviceRegistryPropertyW(
            set,
            d,
            8,
            class.as_ptr() as *const u8,
            (class.len() * 2) as u32,
        )
    })?;
    let mut p: Parameters = unsafe { std::mem::zeroed() };
    p.size = 584;
    check(unsafe { SetupDiGetDeviceInstallParamsW(set, d, &mut p) })?;
    p.flags |= 0x10000;
    p.flags_ex |= 0x800;
    let inf = if enabled { "winusb.inf" } else { "input.inf" };
    let path = wide(&format!(
        "{}\\INF\\{inf}",
        std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into())
    ));
    if path.len() > 260 {
        return Err("Windows 路径过长".into());
    }
    p.path[..path.len()].copy_from_slice(&path);
    check(unsafe { SetupDiSetDeviceInstallParamsW(set, d, &mut p) })?;
    check(unsafe { SetupDiBuildDriverInfoList(set, d, 1) })?;
    let result = (|| {
        for index in 0..2048 {
            let mut drv: Driver = unsafe { std::mem::zeroed() };
            drv.size = 1568;
            if unsafe { SetupDiEnumDriverInfoW(set, d, 1, index, &mut drv) } == 0 {
                break;
            }
            let mut detail = vec![0u8; 16384];
            detail[..4].copy_from_slice(&1584u32.to_le_bytes());
            let mut size = 0;
            check(unsafe {
                SetupDiGetDriverInfoDetailW(set, d, &mut drv, detail.as_mut_ptr(), 16384, &mut size)
            })?;
            let section = read_wide(&detail[32..544]);
            let filename = read_wide(&detail[544..1064]);
            let hw = read_wide(&detail[1576..]);
            let offset = u32::from_le_bytes(detail[12..16].try_into().unwrap()) as usize;
            let len = u32::from_le_bytes(detail[16..20].try_into().unwrap());
            let compatible = if len > 0 && 1576 + offset * 2 < detail.len() {
                read_wide(&detail[1576 + offset * 2..])
            } else {
                String::new()
            };
            let matched = filename.to_ascii_lowercase().ends_with(inf)
                && if enabled {
                    section.eq_ignore_ascii_case("WINUSB")
                } else {
                    section.eq_ignore_ascii_case("HID_Inst")
                        && (hw.eq_ignore_ascii_case("GENERIC_HID_DEVICE")
                            || compatible.eq_ignore_ascii_case("GENERIC_HID_DEVICE"))
                };
            if matched {
                let mut reboot = 0;
                check(unsafe { DiInstallDevice(0, set, d, &mut drv, 0, &mut reboot) })?;
                return Ok(reboot != 0);
            }
        }
        Err("未找到 Windows 自带的接收器驱动".into())
    })();
    unsafe {
        SetupDiDestroyDriverInfoList(set, d, 1);
    }
    result
}
