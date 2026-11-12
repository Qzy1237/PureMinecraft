================================================================
PureMinecraft 1.0.0 —— 极简全功能 MC 启动器
================================================================

一、实测结论（2026-11-08，本机跑通）
----------------------------------------------------------------
  ✔ 获取版本清单：BMCLAPI 超时时自动回退 Mojang 官方
  ✔ 一键安装 1.20.1：3371 个文件，0 失败，约 2 分钟
       client.jar 22 MB / libraries 64 个 / assets 3757 个对象
  ✔ 启动游戏：javaw pid 拉起，游戏进入主菜单
       窗口标题 "Minecraft 1.20.1"，主菜单截图见 shot_game.png
  ✔ 离线账户：昵称 Steve，离线 UUID（MD5 v3）
  ✔ 内存参数：-Xmx4096M

二、目录结构
----------------------------------------------------------------
PureMinecraft\
  .minecraft\              游戏根目录（标准结构）
    versions\1.20.1\       版本 json + client.jar + natives\
    libraries\             依赖库
    assets\indexes|objects 资源索引与对象
  java\jdk-17.0.2\         自带 JDK 17（OpenJDK，华为镜像下载）
  bin\PureMinecraft.exe    主程序
  config\lang\             语言包
  src\                     前端（HTML/CSS/JS）
  src-tauri\               后端（Rust）
  tools\                   构建脚本 / 图标
  dist\PureMinecraft-Setup-1.0.0.exe

三、界面
----------------------------------------------------------------
无边框 + 液态玻璃 + 可拖拽；四个标签页对标 PCL2：
  启动      昵称 / Java 路径 / 内存滑块（2-8G）/ 大号启动按钮
  版本管理  列出 .minecraft\versions 下的本地版本
  资源下载  原版列表（BMCLAPI）+ 自定义版本号安装 + Fabric 面板
  设置      暗黑/纯白主题、语言、镜像说明、Java 与目录状态、卸载
底部日志栏实时滚动（时间戳 + 成功/失败着色）。

四、下载逻辑（踩坑后的最终版）
----------------------------------------------------------------
版本 JSON：BMCLAPI /version/{id}/json，失败则从 manifest 里查官方真实地址
  （注意：官方没有 /version/{id}/json 这个端点，直接拼 URL 一定 404）
client.jar / libraries / assets：每个文件都带双源，逐个试
超时：连接 6s、接收 25s，避免一个死链拖住整个队列
并发：24 线程

实测踩到的坑：
  bmclapi 的 /assets/* 和 /maven/* 在这台机器上全部超时，
  而 resources.download.minecraft.net 587ms 就能返回。
  最初写死 BMCLAPI、没有回退 → 每分钟只能下 4 个文件（3600 个要 14 小时）。
  改成「官方优先、BMCL 兜底」后 → 45 秒 1986 个文件。
  国外用户把顺序反过来即可（代码里是 vec![官方, BMCL]）。

五、启动逻辑
----------------------------------------------------------------
  1. 读 version.json；Fabric 版本按 inheritsFrom 合并父版本
  2. classpath = client.jar + 所有 libraries（跳过 natives 分类器）
  3. natives 解压到 versions\<id>\natives，拼 -Djava.library.path
  4. 离线 UUID = MD5("OfflinePlayer:<名字>")，按 UUID v3 规则改版本位
  5. 参数：-Xmx{内存}M、--username、--uuid、--assetsDir、--assetIndex
  6. Command::spawn 拉起 javaw

Java 查找顺序：
  安装目录\java\jdk-*\bin\javaw.exe → 工程目录\java → PATH

六、构建与打包
----------------------------------------------------------------
  cd E:\Projects\PureMinecraft
  powershell -ExecutionPolicy Bypass -File tools\build.ps1

产物：bin\PureMinecraft.exe + dist\PureMinecraft-Setup-1.0.0.exe

七、模组资源库
----------------------------------------------------------------
新增「模组库」标签页，数据源 **Modrinth**（免费开放 API，无需 Key）：
  GET https://api.modrinth.com/v2/search?query=..&facets=[["project_type:mod"],["categories:fabric"]]
卡片展示：模组图标 / 名称 / 作者 / 简介 / 下载量 / 评分(follows) / 分类
支持：关键词搜索、Fabric/Forge/NeoForge/Quilt 筛选、按下载量或评分排序、滚动加载更多

安装逻辑：
  1. GET /v2/project/{id}/version?game_versions=["1.20.1"]&loaders=["fabric"]
  2. 取 primary 文件，下载到 .minecraft\mods\
  3. 下载完比对官方 size，不一致就删掉重试（最多 3 次）

实测（2026-11-08）：
  · 搜索 "mod" + Fabric → 19049 个结果，图标/简介/下载量全部正常渲染
  · 安装 Fabric API → fabric-api-0.92.12+1.20.1.jar，2087 KB，与官方 size 完全一致

踩坑：
  · Modrinth 的文件地址会 302 到 cdn.modrinth.com，WinHTTP 默认不跟随重定向
    → 打开 WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS
  · 第一次下载只拿到 857/2087 KB 却报了成功（没有大小校验）
    → 加 size 比对 + 最多 3 次重试

CurseForge：官方 API 需要 Key，且这台机器上 api.curseforge.com 直接超时，
所以暂未接入（代码结构留好了，拿到 Key 加个命令即可）。

八、已知限制（如实说）
----------------------------------------------------------------
· Forge 未实现（只做了 Fabric；Forge 需要跑 installer.jar，本轮没排期）
· Fabric 面板需要手动填 loader 版本号，没做版本下拉
· 只有正版登录的骨架（离线可用，微软登录未接）
· 下载不支持断点续传；中断后重跑会跳过已存在的文件
· assets 校验只查文件是否存在，没做 SHA1 校验
· 4GB 内存是默认值，机器内存不足时请调小
================================================================

================================================================
十、自适应改造（2026-11-08，重点）
================================================================
之前所有路径都写死在 E:\Projects\PureMinecraft，换盘符就废。现在全部改成
**以启动器自身位置为锚点**：

  base_dir() = PureMinecraft.exe 所在目录的上一级（如果在 bin\ 里）

  .minecraft   = <base>\.minecraft
  versions\    = <base>\.minecraft\versions
  libraries\   = <base>\.minecraft\libraries
  assets\      = <base>\.minecraft\assets
  uninstall.exe= <base>\uninstall.exe
  config\lang\ = <base>\config\lang\

Java 查找顺序（find_java）：
  1. <base>\runtime\bin\javaw.exe
  2. <base>\java\*\bin\javaw.exe     （多个 JDK 目录都能认）
  3. <base>\jre\*\bin\javaw.exe
  4. <base>\runtime\*\bin\javaw.exe
  5. 系统 PATH 里的 javaw.exe / java.exe（用 where 探测）

扫到就**自动填入 UI 的 Java 路径框**，不用手动点。

找不到时弹窗：
  「未检测到运行环境 … 是否立即为您自动下载？」
  点「立即下载」→ 从华为镜像拉 OpenJDK 17（约 178 MB）
  → 解压到 <base>\runtime\ → 自动回填路径

实测：
  ✔ 同一份程序放到 D:\PortableTest\PureMinecraft\
      → 日志：启动器目录 D:\PortableTest\PureMinecraft
              游戏目录   D:\PortableTest\PureMinecraft\.minecraft
              Java       D:\PortableTest\PureMinecraft\runtime\bin\javaw.exe
      → 点启动，Minecraft 正常跑起来（Fabric 1.20.1 + 两个模组都加载）
  ✔ 放回 E:\Projects\PureMinecraft\bin\
      → 自动识别 java\jdk-17.0.2\bin\javaw.exe 和 .minecraft
  ✔ 把 runtime 改名（模拟没装 Java）
      → 弹出「未检测到运行环境，是否立即为您自动下载？」（截图 shot_nojava.png）

便携用法：整个 PureMinecraft 文件夹（bin / config / resources / java 或 runtime /
.minecraft）拷到任何盘、任何目录，双击 bin\PureMinecraft.exe 即可。

================================================================
================================================================
十一、模组详情页（二级菜单，2026-11-10 新增）
================================================================
在「模组库」里点任意一张卡片，进入二级详情页。

页面结构：
  面包屑   模组库 / <模组名>            ← 左上角返回箭头
  头部     图标 + 名称 + 作者 + 下载/关注/分类/加载器 + 【安装】按钮
  标签页   简介 / 图集 / 宣传视频 / 评论

四个标签：
  简介      渲染 Modrinth 的 body（简易 markdown：标题/列表/粗体/链接/图片/代码块）
  图集      项目 gallery 图片，两列网格
  宣传视频  从简介正文里识别 YouTube / Bilibili 链接，识别到就内嵌 iframe 播放
            识别不到就给出「在 B 站搜索 / 在 YouTube 搜索」按钮（调系统浏览器）
  评论      Modrinth 没有公开评论接口，如实展示关注数/下载数 +
            GitHub Issues / 源码 / Modrinth 项目页 三个外链

后端命令：
  mod_detail(project_id)              GET /v2/project/{id}
  mod_versions(project_id, mc, loader) GET /v2/project/{id}/version?...  （预留）
  open_url(url)                        用系统浏览器打开（只允许 http/https）

实测：
  ✔ 点 Fabric API 卡片 → 详情页打开，简介完整渲染（标题/列表/链接）
  ✔ 评论页显示「Modrinth 没有公开的评论接口」+ 关注数 37.7K / 下载 285.4M
  ✔ 宣传视频页对 Iris Shaders 给出「简介里没有内嵌视频」+ 两个搜索按钮
  ✔ 安装按钮在详情页右上角，点一下直接装当前选中版本对应的 jar

修过的坑：
  · 详情页刚做出来时【安装】按钮被 flex 挤成竖排 → 加 flex:0 0 auto + nowrap
  · 公告区的链接是默认深蓝色，在深色底上看不清 → 加 .notice a 用主题色
  · markdown 的 <li> 每个都单独包 <ul>，导致行距巨大 → 合并连续 li 到一个 ul

================================================================
================================================================
十二、卸载时可选是否删除存档（2026-11-11 新增）
================================================================
以前卸载器是 rmdir /s /q 整个安装目录，.minecraft 里的存档一起没。
现在改成默认保留，只有明确选择才删。

界面：设置 → 卸载 PureMinecraft → 弹窗
  ┌──────────────────────────────────────────┐
  │ 卸载 PureMinecraft                        │
  │ 存档目录：<base>\.minecraft\saves         │
  │ 共 N 个世界，占用 XX MB（列出前 5 个名字） │
  │ ○ 只卸载程序   保留 .minecraft            │
  │ ○ 连存档一起删除  删掉 .minecraft 整个目录 │
  │                        [取消] [确认卸载]  │
  └──────────────────────────────────────────┘

卸载器命令行：
  uninstall.exe            只删程序（bin/config/resources/src/src-tauri/tools/
                           stage/dist/java + 根目录 *.txt *.png *.log + 自己）
                           保留 .minecraft
  uninstall.exe /purge     再把 .minecraft 整个删掉

实测（拿精简目录做的两组对照）：
  不带 /purge → bin/config/README/uninstall.exe 全删；
                .minecraft\saves\MyWorld\level.dat 原样保留（内容校验一致）✅
  带 /purge   → 整个目录消失，level.dat 不存在 ✅

注意：卸载器仍然会删桌面快捷方式（这是标准行为）。
      如果你只是想重建快捷方式而不是卸载，别点确认。
================================================================
================================================================
十三、跨机器可移植性（2026-11-11 清理）
================================================================
这一轮把"只适配这台机器"的东西全部清掉了：

1) 安装脚本 installer.nsi
   · 安装目录从 "$LOCALAPPDATA\Programs\PureMinecraft (Rust 1.99)"
     改成 "$LOCALAPPDATA\Programs\PureMinecraft"（后缀是构建残留）
   · 删除残留的 PUREDesk 文案（"桌面布局备份工具…"）
   · 删除残留的硬编码 CreateDirectory "E:\Projects\PureMinecraft\backups"
   · 删除残留的 PureDocs 右键菜单（"强制解除占用并粉碎" / --shred）
   · 删掉没用到的 NSIS Uninstall 段（我们自带 uninstall.exe）

2) 构建脚本 build.ps1
   · 去掉 $LangTag = 'Rust 1.99'
   · Rust / MinGW 路径改成"存在才用，否则走系统 PATH"：
       if (Test-Path 'E:\Rust\cargo') { CARGO_HOME = ... }
   · VS 候选目录保留 C 盘回退
   · .nsi 必须带 UTF-8 BOM，否则 makensis 报 Bad text encoding

3) 程序本体（之前已经做过，这里复核）
   · base_dir() 由 exe 位置推导，无任何写死盘符
   · Java：runtime\bin → java\*\bin → jre\*\bin → PATH
   · 找不到就弹窗自动下载 OpenJDK 17（华为镜像）

实测（全新目录静默安装）：
  ✔ Setup.exe /S /D=D:\InstallTest\PureMinecraft
      → InstallLocation = D:\InstallTest\PureMinecraft
      → 无右键菜单键、无 backups 目录
  ✔ 该安装启动后：启动器目录 D:\InstallTest\PureMinecraft
                  游戏目录   D:\InstallTest\PureMinecraft\.minecraft
                  Java 未找到 → 弹「是否立即自动下载」
  ✔ 点下载：177.6 MB / 21 秒下完，解压出 runtime\bin\javaw.exe（总耗时 36 秒）
      → UI 自动回填 D:\InstallTest\PureMinecraft\runtime\bin\javaw.exe
  ✔ 默认安装：C:\Users\<用户>\AppData\Local\Programs\PureMinecraft
      → 注册表 DisplayName = PureMinecraft（无后缀）

仍与机器相关但无害的：
  · 构建脚本里 E:\ 的工具链路径只是"优先尝试"，别的机器会自动退回 PATH
  · localStorage 按 app id 共享，同一台机器上放多份便携版会共用设置（不影响换电脑）
================================================================