# SaloHUb

面向 Windows 10 / 11 的小米、海信遥控器控制工具，由 **MingDa9916** 维护。

[项目仓库](https://github.com/MingDa9916/SaloHub) · [下载安装包](https://github.com/MingDa9916/SaloHub/releases) · [反馈问题](https://github.com/MingDa9916/SaloHub/issues)

## 当前版本

当前版本 `1.0.1`：UWB 精简为原始触摸 / 长按移动两种模式，修复快捷键映射识别；新增默认开启的荧光笔、按键开关及颜色、粗细、时长、不透明度、发光强度和四种预设。既有触摸、防抖、慢移精度、语音和高级按键映射保留。输入法弹窗与小米统一，托盘仅保留打开面板和退出。海信只显示真实充电电源连接状态。新增命令库、完整按键库、四种运行方式的宏、按应用自动切换配置、固定配置及可绑定的配置切换。左上角按键动作与状态提示支持统一开关、背景与文字颜色、默认 50% 的背景透明度。

本项目公开发布源码与 Windows x64 安装包，无需登录即可下载。程序从 SaloHub 官方发布页和更新清单检查更新。

## 支持设备

| 设备 | 连接与功能 |
| --- | --- |
| 小米蓝牙遥控器 2 Pro | 蓝牙 BLE、按键映射、ATVV 语音输入 |
| 海信灵指 CRF3C65 / DRF3U65 | USB 接收器、UWB 指向、触屏点按和滑动、语音输入、独立按键映射 |

实际可映射的海信按键取决于接收器是否收到数据；只发送红外的按键需要硬件支持，软件无法直接接收红外。

## 海信控制

- 模式切换动作循环两种 UWB 模式：原始触摸、长按移动，可映射到 AI、快捷等已识别按键。
- 触摸圆点、按下变色扩散、防抖及慢移精细瞄准。
- 单击、双击、三击、四连击、独立长按；支持键盘组合、文字、应用启动、鼠标操作和语音输入。
- 实物遥控器预览与映射列表联动，支持按键学习、手动组合快捷键和映射导入导出。
- 命令 / 按键 / 宏与清除 / 恢复 / 学习统一排列；单键恢复保留其他按键及设置，应用独立映射可恢复继承。
- 设置自动保存；暂停、断连和快捷键录入时释放输出并取消待触发动作。

[海信使用与联调记录](docs/HISENSE.md) · [映射逻辑说明](docs/HISENSE-MAPPING.md)

## 安装

1. 从本项目 Releases 下载 Windows x64 安装程序并运行。
2. 原有用户选择覆盖升级；程序配置继续使用现有配置目录。
3. 插入海信 USB 接收器，在“海信灵指”页连接。首次启用独立映射可能需要确认管理员提示。
4. 使用语音输入时配置 VB-CABLE 与所用输入法的语音快捷键。声音有电平不代表输入法已完成识别。

程序需要 Microsoft Edge WebView2 Runtime。海信接收器管理仅调整对应接收器输入接口，麦克风仍使用系统音频驱动。

## 配置与宏

- 桌面默认提供通用映射；在“管理游戏与应用配置”添加 .exe 进程名，为应用独立配置按键，未单独配置的按键继承默认。
- 可启用按应用自动切换或固定使用一个配置；切换时左上角提示，同一应用内切换窗口不重复提示。
- 在遥控器下方编辑区打开“命令 / 按键 / 宏”。宏支持不重复、按住时重复、触发切换、按下 / 按住 / 松开序列，支持键盘录制、鼠标步骤、延迟与颜色。
- 宏在应用切换、断连、设置变更或录制开始时停止并释放输入；宏列表提供“停止所有宏”。
- 保留原应用数据标识，升级继续读取已有指向、语音和映射配置。

## 开发与构建

需要 Node.js 20+、Rust stable MSVC、Visual Studio C++ Build Tools 和 WebView2。

```powershell
git clone https://github.com/MingDa9916/SaloHub.git
cd SaloHub
npm ci
npm run tauri:dev
```

```powershell
npm test
cargo check --locked --all-targets --manifest-path src-tauri/Cargo.toml
npm run tauri -- build --bundles nsis
```

PowerShell 如阻止 npm 脚本，可将 `npm` 改成 `npm.cmd`。安装包输出至 `src-tauri/target/release/bundle/nsis/`。

## 开源许可

本项目基于 [LightyearXizIl/Nexus-Prime](https://github.com/LightyearXizIl/Nexus-Prime) 适配开发，保留原项目历史、版权声明与 [MIT 许可证](LICENSE)。第三方组件的许可及分发说明见 [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) 和 `third-party-licenses/`。
