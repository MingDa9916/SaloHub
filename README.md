# Nexus Prime · 海信增强版

面向 Windows 10 / 11 的小米、海信遥控器控制工具，由 **MingDa9916** 维护。

[项目仓库](https://github.com/MingDa9916/Nexus-Prime-Hisense) · [下载安装包](https://github.com/MingDa9916/Nexus-Prime-Hisense/releases) · [反馈问题](https://github.com/MingDa9916/Nexus-Prime-Hisense/issues)

## 当前版本

`0.5.6-hisense.4`：完善支持设备和维护者信息，移除赞赏与旧项目展示入口；包含已集成的海信 UWB、触摸、语音和高级按键映射功能。

仓库为私有时，需要登录有访问权限的 GitHub 账号下载安装包。应用中的“查看更新与安装包”直接打开本项目发布页，不会下载原项目的安装包。

## 支持设备

| 设备 | 连接与功能 |
| --- | --- |
| 小米蓝牙遥控器 2 Pro | 蓝牙 BLE、按键映射、ATVV 语音输入 |
| 海信灵指 CRF3C65 / DRF3U65 | USB 接收器、UWB 指向、触屏点按和滑动、语音输入、独立按键映射 |

实际可映射的海信按键取决于接收器是否收到数据；只发送红外的按键需要硬件支持，软件无法直接接收红外。

## 海信控制

- AI 键循环三种 UWB 模式：原始触摸、长按移动、长按滑动。
- 触摸圆点、按下变色扩散、防抖及慢移精细瞄准。
- 单击、双击、三击、四连击、独立长按；支持键盘组合、文字、应用启动、鼠标操作和语音输入。
- 实物遥控器预览与映射列表联动，支持按键学习、手动组合快捷键和映射导入导出。
- 设置自动保存；暂停、断连和快捷键录入时释放输出并取消待触发动作。

[海信使用与联调记录](docs/HISENSE.md) · [映射逻辑说明](docs/HISENSE-MAPPING.md)

## 安装

1. 从本项目 Releases 下载 Windows x64 安装程序并运行。
2. 原有用户选择覆盖升级；程序配置继续使用现有配置目录。
3. 插入海信 USB 接收器，在“海信灵指”页连接。首次启用独立映射可能需要确认管理员提示。
4. 使用语音输入时配置 VB-CABLE 与所用输入法的语音快捷键。声音有电平不代表输入法已完成识别。

程序需要 Microsoft Edge WebView2 Runtime。海信接收器管理仅调整对应接收器输入接口，麦克风仍使用系统音频驱动。

## 开发与构建

需要 Node.js 20+、Rust stable MSVC、Visual Studio C++ Build Tools 和 WebView2。

```powershell
git clone https://github.com/MingDa9916/Nexus-Prime-Hisense.git
cd Nexus-Prime-Hisense
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
