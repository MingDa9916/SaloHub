<script setup lang="ts">
import { computed, ref, watch } from 'vue';
const props=defineProps<{open:boolean}>();const emit=defineEmits<{close:[]}>();
const search=ref(''),category=ref('全部');
const articles=[
 {group:'入门',title:'首次连接海信遥控器',text:'插上 CRF3C65 / DRF3U65 接收器，打开海信灵指页并点击连接接收器。设备概览显示接收器连接、指向模式、最近按键与充电状态。若连接失败，打开接收器管理查看驱动状态；只对海信接收器操作。使用 UWB 前请开启遥控器自身的指向功能。'},
 {group:'入门',title:'小米遥控器与服务状态',text:'先在 Windows 蓝牙设置中配对小米遥控器，再到小米遥控页连接。服务状态分别显示虚拟声卡、语音路由、按键桥接和键盘注入。遇到异常时使用对应修复入口，等待完成后再重试，实时动态会显示处理结果。'},
 {group:'指向与触摸',title:'模式 1/2：原始触摸',text:'圆点始终跟随遥控器。确定键按下即开始触摸，松开抬起；按住并移动可拖动。与已发布 1.0.2 相同。两种模式只影响设置为“确定键（随模式）”的按键，自定义为空格等其他功能时按映射执行。'},
 {group:'指向与触摸',title:'模式 2/2：长按移动',text:'平时圆点不移动。短按确定在当前位置点按；长按超过设定阈值后只移动圆点，不保持触摸、不拖动，松开停止。给任意已识别的按键分配“循环切换两种模式”即可切换，左上角显示当前模式。'},
 {group:'指向与触摸',title:'灵敏度、防抖与精细点按',text:'打开指向与防抖，调整移动灵敏度、慢移自动降速、慢移倍率、防抖强度、微动过滤和长按阈值。慢速瞄准时降低倍率，快速移动恢复正常灵敏度。防抖过强会增加跟随延迟，建议小幅调整。触摸圆点可独立开启并选择普通及按下颜色。'},
 {group:'指向与触摸',title:'荧光笔轨迹',text:'荧光笔默认开启，移动时留下短暂光迹并自动消失。可选择预设、颜色、粗细、持续时间、不透明度和发光强度。映射“荧光笔开关”可用遥控键切换，左上角显示开关状态。轨迹不修改屏幕内容。'},
 {group:'按键映射',title:'选择按键和触发方式',text:'点击遥控器预览热点或右侧列表，编辑区、当前选择和列表会同步。先选择桌面默认或某个应用，再设置单击、双击、三击、四连击、长按。连击间隔控制识别窗口。仅设置单击时尽快响应；设置连击后短按会等待判断次数。'},
 {group:'按键映射',title:'命令、按键与手动组合',text:'命令库提供 Windows、编辑、导航、媒体和遥控器功能。按键库可搜索字母、数字、标点、方向、F1–F24、小键盘及修饰键。录入会捕获真实键盘组合，手动组合无需按出系统快捷键；支持 Shift 标点和附加普通键。它们都在“按键”窗口内。'},
 {group:'按键映射',title:'清除、恢复、学习有什么区别',text:'清除：本次触发不执行动作。恢复：桌面按键恢复出厂动作及触发方式；应用按键恢复继承桌面默认。学习：将接收器实际发出的键码绑定到某个实体键，仅首次识别或纠正串位时使用；已识别按键修改功能后不用再学习。只发红外且接收器无法接收的键，软件不能凭空识别。'},
 {group:'按键映射',title:'实时动作与长按',text:'语音输入、按住拖动、随模式确定以及部分宏需要持续按下与松开事件，因此不能同时配置独立连击。UWB 模式切换设置在基础单击动作。当前按键实际执行的动作会在左上角提示，连发不会不断堆叠提示。'},
 {group:'应用配置',title:'添加游戏和应用程序',text:'打开应用配置管理，点击添加，会自动扫描本机应用并按名称排序。可以搜索名称、进程或路径，切换按来源排序；点击选中后保存。扫描包含开始菜单、桌面快捷方式、注册应用、安装目录、商店应用、Steam 和运行中程序。未注册的便携应用仍可浏览选择或填写进程名。'},
 {group:'应用配置',title:'自动切换、固定与继承',text:'配置按前台应用的 exe 进程名自动匹配。未独立配置的按键继承桌面默认。关闭配置文件切换后统一使用桌面默认；固定配置会覆盖自动匹配。可把选择某配置、恢复自动匹配、循环切换配置映射给遥控器按键。切换时左上角提示，旧按键和宏会释放。'},
 {group:'宏',title:'创建宏和四种运行方式',text:'在“宏”中创建：不重复执行一次；按住时重复在松开后停止；触发模式按一次开始、再按一次停止；序列模式分为按下、按住循环、松开三个阶段。宏可以分配给多个按键和应用。循环宏请分配一个“停止所有宏”按键以便随时结束。'},
 {group:'宏',title:'录制、步骤与延迟',text:'键盘录制会替换当前阶段，停止后可增删、排序步骤。可手动添加键盘按下/抬起、鼠标点按、延迟及启动应用。每阶段最多 256 步；单步延迟最多 60 秒。使用标准延迟时，操作之间使用统一延迟，录制的单独延迟不生效。关闭标准延迟可逐步调整时间。'},
 {group:'宏',title:'可视化编辑与导出',text:'在映射与宏编辑器中点击遥控器按键，选择触发方式，再选键盘、命令、宏或应用等动作。在宏库中可添加、排序和删除步骤。完成后点击“应用更改”。独立宏库导出只带宏；全部映射导出带按键、应用配置和宏，均不包含设备学习、麦克风、指向与个人外观设置。'},
 {group:'宏',title:'迁移到另一台电脑与应用自动关联',text:'在新电脑打开映射与宏编辑器，导入之前导出的配置文件。软件扫描本机应用，用 exe 文件名寻找启动动作和宏中的应用，并显示关联结果，不会在导入时运行应用。唯一匹配会自动替换为本机路径；未安装或有多个同名程序时，点击选择对应应用。关联并核对后应用。应用配置仍按进程名匹配；独立宏按 ID 合并，同 ID 更新。'},
 {group:'语音',title:'输入法设置与语音键',text:'输入法快捷键必须与输入法自身设置一致。千问提供四种按住说话组合；微信可选按住说话或启动语音；豆包可选长按或免按。应用预设后，遥控器语音键发送对应组合。海信可选择麦克风并调节增益，任何支持的按键都可映射为语音输入。'},
 {group:'语音',title:'语音开始音、结束音',text:'全局设置的语音提示音支持开关、10 组预设、自定义歌曲、试听、音量和片段裁剪。提示音通过本机播放设备输出，海信和小米共用设置。先试听再调节音量，以实际麦克风和输入法识别结果为准。'},
 {group:'外观',title:'左上角弹窗个性化',text:'海信页映射列表下方可开关弹窗，设置背景透明度、背景颜色与文字颜色，并查看预览。默认背景透明度 50%，文字保持清晰。模式切换、荧光笔、应用切换与按键动作使用统一设置。关闭后不再显示新提示。'},
 {group:'外观',title:'窗口、托盘、启动与主题',text:'右侧三个彩色按钮依次为最小化、最大化/还原、关闭；拖动标题栏可移动窗口，双击可最大化。关闭是否进入托盘由全局设置控制。托盘只保留打开面板和退出；要完全关闭请点退出。全局设置可配置开机启动、启动隐藏、主题、语言和日志保留。'},
 {group:'故障排查',title:'按键串码、重复输入与紧急停止',text:'未知接收器键码不会再直接输入数字。异常或超时的按键状态会释放，丢失松开时避免无限输入。出现异常可先断开接收器桥接，或从宏库停止所有宏，然后退出重连。遥控器原生红外和键盘本身不被全局改键。长按持续超过 8 秒且没有新的键盘报告时会自动释放，请松开后重按。'},
 {group:'故障排查',title:'电量、充电与未识别按键',text:'海信接收器尚无已验证的电量百分比字段，因此显示充电电源已连接、未连接或待更新，不估算电量。充电电源连接不等于充满。没有收到数据的实体键可尝试学习；仍无数据时需要硬件支持。恢复接收器原驱动后，独立映射可能暂时不可用。'},
 {group:'故障排查',title:'保存、更新与 GitHub',text:'常规设置自动保存在本机；编辑器里的更改在点击“应用更改”后生效。导入格式错误保留现有配置。检查更新和安装程序在关于页面中；源码改动不会自动同步 GitHub，需要提交并推送。此次预览在你确认后才更新仓库。'},
];
const groups=['全部',...new Set(articles.map(a=>a.group))];
const rows=computed(()=>articles.filter(a=>(category.value==='全部'||a.group===category.value)&&`${a.group} ${a.title} ${a.text}`.toLowerCase().includes(search.value.trim().toLowerCase())));
watch(()=>props.open,o=>{if(o){search.value='';category.value='全部';}});
</script>
<template><div v-if="open" class="manual-mask" @click.self="emit('close')" @keydown.esc.stop="emit('close')"><section class="manual" role="dialog" aria-modal="true" aria-label="遥控器说明书"><header><div><h2>遥控器说明书</h2><p>SaloHUb · 海信与小米 · 功能查询和使用教程</p></div><button aria-label="关闭说明书" @click="emit('close')">×</button></header><div class="manual-search"><input v-model="search" autofocus aria-label="搜索说明书" placeholder="搜索功能或问题，例如：长按、导出、语音、串码…" /><small>{{ rows.length }} 篇</small></div><div class="manual-body"><nav><button v-for="g in groups" :key="g" :class="{active:category===g}" @click="category=g">{{ g }}</button></nav><main><details v-for="article in rows" :key="article.title" :open="!!search"><summary><small>{{ article.group }}</small>{{ article.title }}</summary><p>{{ article.text }}</p></details><p v-if="!rows.length">没有找到结果，试试更短的关键词。</p></main></div></section></div></template>
<style scoped>
.manual-mask{position:fixed;inset:0;z-index:3700;background:var(--overlay);display:grid;place-items:center;padding:24px}.manual{width:min(1000px,94vw);height:min(790px,88vh);background:var(--card-bg);color:var(--text);border:1px solid var(--border);border-radius:16px;display:flex;flex-direction:column;overflow:hidden;box-shadow:var(--dialog-shadow)}header{padding:22px 24px;display:flex;align-items:center;justify-content:space-between;border-bottom:1px solid var(--border)}h2{margin:0;font-size:21px}header p{margin:7px 0 0;font-size:12px;color:var(--text-secondary)}button,input{font:inherit;color:var(--text);background:var(--card-bg);border:1px solid var(--border);border-radius:9px;padding:9px 12px}button{cursor:pointer}.manual-search{display:flex;align-items:center;gap:15px;padding:20px 24px}.manual-search input{flex:1;min-width:0;font-size:13px}.manual-search small{font-size:12px;color:var(--text-secondary)}.manual-body{display:flex;min-height:0;flex:1}nav{width:140px;padding:0 14px 20px 24px;display:flex;flex-direction:column;gap:7px;overflow:auto}nav button{text-align:left;font-size:12px}nav button.active{color:var(--primary);border-color:var(--primary);background:var(--surface-soft)}main{flex:1;overflow:auto;min-width:0;padding:0 24px 24px 12px}details{border:1px solid var(--border);border-radius:10px;margin-bottom:10px}summary{cursor:pointer;padding:16px;font-size:14px;font-weight:600}summary small{color:var(--primary);font-size:10px;margin-right:10px}details p{padding:0 18px 15px;margin:0;line-height:1.95;font-size:13px;color:var(--text-secondary)}@media(max-width:650px){nav{width:100px;padding-left:12px}main{padding-right:12px}.manual-mask{padding:10px}.manual{width:97vw}}
</style>
