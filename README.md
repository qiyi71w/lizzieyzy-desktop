# LizzieYzy Tauri

A desktop Go (weiqi) review workbench being migrated from the Java
[LizzieYzy Next](https://github.com/wimi321/lizzieyzy-next) (reference commit
`cb53acc55eda42c6c671b5f668cd3b597456bf07`) to Tauri 2 + Rust + TypeScript. The Java program's
behaviour, defaults and workbench layout are the reference; this client keeps its own identity
and data directory. It only reads the Java configuration, and only when the user imports it.

## Current state

Implemented: opening, browsing, editing and saving records, analysis with one local GTP engine
(KataGo, Leela Zero, Sayuri, Sai or Leela 0.11, recognised by their GTP name like Java), and
importing the matching LizzieYzy Next settings.

- Open SGF and GIB files with the File menu, the toolbar button, `O`, a dropped file, a command
  line argument, or File > recent records. Paste an SGF record with `Ctrl+V` (it asks before
  replacing a record that has content).
- Browse with the Java keys and buttons: `Up`/`Down`, `Ctrl+Up`/`Ctrl+Down` and
  `PageUp`/`PageDown` (10 moves), `Home`/`End`, `Left`/`Right` (sibling branches),
  `Ctrl+Left` (branch point), `Shift+Up` (branch start), `Ctrl+Shift+Up`/`Down` (previous/next
  comment), `B` (back to the main branch), the mouse wheel, the bottom bar navigation buttons and
  move-number field, and clicks in the variation tree. The variation tree scrolls with its own
  scroll bars and mouse wheel; it re-centres on the current node when the position changes.
- Stepping forward plays the Java stone sounds (stone, capture, capture of three or more).
- The board shows setup stones, moves, passes, captures, comments, SGF markup (TR/SQ/CR/MA/LB),
  next-move rings, player names, the sub board, the variation tree and the empty winrate graph.
- A failed or cancelled open keeps the current record. Opening a 9x9 record after a 19x19 one (or
  back) replaces the board completely.
- Save with File > 保存, the toolbar save button or `Ctrl+S`: a record that came from (or was
  saved to) a file overwrites that same file, under its own name, after the Java "覆盖原文件?"
  prompt (Cancel is the default; "不再提示" turns it off, View > 主界面设置 >
  显示覆盖文件提示对话框 and View > 重置界面位置 turn it back on). A new, pasted or GIB record
  goes to the save chooser instead.
- Save As with File > 另存为 or `S`: the system save chooser opens in the last save folder (at
  first the home folder, Documents on Windows) with the Java default name (`<black>_Vs_<white>`
  or 未命名, then a timestamp). `.sgf` is added to a chosen name without it; if that file exists
  the Java "SGF文件已经存在" prompt asks first (the system chooser itself asks about a file
  picked directly). The record then belongs to the new file.
- Records are always written as UTF-8 SGF (GIB is read-only, as in Java). Setup stones, the side
  to play, real and placeholder passes, branches, comments, markup, `MN` and the saved analysis
  properties (`LZ`, `LZ2`, `LZOP`, `LZOP2`) and other unknown properties are kept, and LizzieYzy
  Next reads the files with the same meaning. A file is written to an owner-only temporary file
  next to it and then renamed, so a failed save ("保存失败") leaves the old file untouched; a
  replaced file keeps its permissions. Normal window close and application Quit both wait for
  saves still being written and record the window placement. On macOS, unhandled Command
  shortcuts remain available to the native application menu, including `⌘Q` to quit.
  On Linux, the saved size is the WebView content area, excluding GTK window decorations,
  so reopening does not enlarge the content area by the title bar or shadow dimensions.
  File > 强制退出 skips both the pending-save wait and window-placement capture; use normal
  close or Quit when a save must finish. Both exit paths shut down the owned engines.
- File > 更多保存 has the Java exports, each through the save chooser like 另存为 (same folder,
  default name, `.sgf` completion and replace prompt):
  - 保存纯净棋谱 (`Ctrl+Shift+S`): the whole tree without analysis (`LZ`, `LZ2`) and without move
    comments; the root comment and root analysis stay. The record then belongs to that file but
    stays unsaved, as in Java.
  - 保存纯净棋谱(带评论): the same with every comment; an analysed move (analysis of this session
    or saved in the file, whichever has more visits) gets Java's win-rate summary in its comment,
    in the interface language: the side that moved, or Black with 总是显示黑胜率/目数, and the lead
    as 目数显示 shows it (领先 / 盘面). Unlike Java, the lead's change under 总是显示黑胜率/目数 is
    always Black's own change; Java's sign depended on the position shown when exporting.
  - 保存纯净分支 (`Ctrl+Alt+S`): one line from the root through the current move, then the first
    continuation, without comments.
  - 保存主棋盘截图 (`Alt+S`), 保存小棋盘截图 (`Shift+S`) and 保存胜率图 (`Shift+Alt+S`): the
    picture chooser (PNG, JPG, GIF, BMP) opens in the last picture folder; the chosen name's
    extension decides the format and `.png` is added otherwise. Errors show Java's message.
  Exports never replace the open record's revision or saved state otherwise; a failure leaves
  the old file untouched.
- File > 复制主棋盘到剪贴板 (`Shift+C`), 复制小棋盘到剪贴板 (`Alt+C`) and 复制棋谱到剪贴板
  (`Ctrl+C`) put the picture or the SGF text on the system clipboard.
- File > 存档与读档 or the toolbar's 存档 button opens Java's save-and-load panel: slots with the
  board picture, name and time; 新建存档, 保存 (asks first), 改名, 删除, 读取 (or a click on the
  picture), 全部删除, and the check boxes 鼠标悬停放大缩略棋盘, 自动存档 and 启动时自动恢复. A loaded
  slot opens as a new unsaved record without a file of its own (saving asks for a name) and the
  title shows the slot. With 自动存档 on (the default) the automatic slot is written every
  60 seconds and when the window closes; File > 强制退出 writes none. If the application ended
  without its exit save, the next start keeps the newest periodic save as the automatic slot.
  With 启动时自动恢复 on, a start without a record to open resumes the automatic slot at the end
  of its main line. Slots live in this program's data folder (`save/`), never in Java's.
- Editing, at the Java entries (Edit and Game menus, the top edit and markup buttons, the bottom
  bar, board clicks, the two board context menus and the Java keys):
  - A board click plays the side to play (交替落子) or, with 添加黑子 / 添加白子, that colour
    (right click the other one). Clicking the point of the next main-line move follows it, an
    equal existing variation is followed, otherwise a new last variation is added. Occupied
    points, single-stone suicide, multi-stone suicide unless the foreground engine's rules allow
    it, and an immediate ko recapture are ignored without changing anything. `P` passes; `,` plays
    the best candidate (or the previewed variation); a middle click plays the previewed variation;
    自动补完征子 continues a ladder.
  - 插入黑子 / 插入白子 (Edit menu toggle or the empty-point context menu) put a move after the
    current one; 反色, 删除 and 移动 on a stone (stone context menu, or dragging with 启用拖动棋子)
    change it where it was played. What followed is played again and keeps its comments, markup,
    setup stones and other properties; stored analysis of the changed positions is dropped.
  - `Delete`/`Backspace` 删除一手 and `Shift+Delete` 删除分支 ask first (Java's "删除" dialog)
    when moves follow; the bottom bar 删除 does not ask. An answer applies only to the record,
    content version and move it was asked for. 撤销删除 / 恢复删除 (shown only when
    possible) and the context menu 撤销编辑 / 恢复编辑 / 清除编辑缓存 undo and redo the last
    delete, insert or stone edit. `Shift+Left`/`Shift+Right` move the current branch, `L` and
    设为主分支 make it the main line, 添加为分支 adds a previewed variation beside the line.
  - Comments: click the comment pane to edit; the text is saved to the node it was opened on when
    another place is pressed or the position changes. While typing, keys never play or navigate.
  - Markup (标记工具, shown when switched on): letters and numbers count on (A, B, … / 1, 2, …),
    triangle, square, cross, the eraser and 清除标记.
  - 起始局面设置 (Game menu): setup mode is available on a record without moves (otherwise it
    asks to turn the current position into the starting position) while no foreground engine
    runs, and no engine starts while it is on. Clicks place black or white setup stones or erase
    them (right click the other colour); 黑先 / 白先 set the side to play (`PL`). Setup stones take
    no move number. 将当前盘面设为起始局面 converts the current position.
  - 编辑棋局信息 (`I`: names, komi), 设置棋盘大小 (`Ctrl+I`; 其他 sizes are remembered), 新建 and
    the bottom bar 清空棋盘 (Java's "新的棋盘?" prompt with 不再提示, Cancel by default),
    清空棋盘 `Ctrl+Home` without asking, 交换黑白 and the rotations / mirrors
    (`Ctrl+Shift+Alt+Right`, `Ctrl+Alt+arrows`; rotation needs a square board), 交换落子顺序
    (changes the side to play when the engine analyses either side, else passes) and 试下 (`V`,
    bottom bar button hidden by default as in Java): the record is kept aside and comes back,
    with the position, when 试下 ends.
  - Every edit of saved content raises the document revision, so edits made while a save runs
    stay unsaved, and analysis is kept for every position an edit did not change.
  - 五子棋无提子规则 (综合设置 > 高级选项, asks "五子棋?" when switched on, off by default):
    moves capture nothing and a point without liberties is a normal move; records opened while
    it is on are read the same way. Positions already placed keep their stones when it is
    switched; it is not a Gomoku rule set (no five-in-a-row detection).
- Settings available now: reading komi from SGF records, stone sounds on/off, the UI language
  (zh-CN, zh-TW, en, ja, ko, th; applied after a restart, like Java), and the editing toggles
  (启用双击找子, 启用拖动棋子, 显示清空棋盘提示对话框, 标记工具, 五子棋无提子规则). A setting that
  cannot be saved is reported and left unchanged.
- Help > 诊断与日志 shows this client's logs, opens the log folder and exports a diagnostics zip;
  Help > 清除所有个人数据 clears the recent-record list; View > 重置界面位置 resets the saved
  window placement and last folder and turns the overwrite prompt back on.
- Engines: the engine list (更多引擎 / `Alt+X`) adds, edits, orders and removes engine profiles
  with Java's fields (name, command with Java quoting, initial commands, board size, komi,
  default engine, preload). The engine at start-up is the default one, the last one, none, or
  chosen from the list. `Ctrl+1`…`Ctrl+0` switch the foreground engine; with fast engine change
  the previous engine keeps running for a quick switch back. Analysis menu items close or
  restart engines. As in Java, an engine's board size applies to a new empty game, and its komi
  becomes the game komi unless the komi was set by hand (an engine that fails to start changes
  nothing).
- Analysis: candidates on the board (win rate, visits, score lead), the variation shown when
  hovering a candidate (captures are worked out by the core), the sub board cycling through
  candidates on click, the candidate tables, the win-rate graph, 统计 and Kata形势 (below), and
  the status and title lines. Candidates use
  Java's wood-backed allocation colours and row layout, including the separate visits font.
  Next-move dashed hints sit behind the labels; hovering a variation hides those hints until
  the pointer leaves. The bottom-bar analysis button uses pause/play
  icons and keeps its width when switching states. `Space` pauses
  and resumes for this session only (never saved); opening another record ends the pause. Time
  and visit limits stop the current position once and resume on the next one. Results stay with
  the position, komi and rules they were requested for, so browsing quickly, switching records,
  changing komi or rules never shows another context's analysis; changing komi or rules discards
  the stored analysis like Java.
- 显示 > 选点信息 > 总是显示黑胜率/目数 and 显示 > KataGo相关设置 > 目数显示 (计算贴目后目数差 /
  盘面目数差) are Java's two shared settings, both off by default, saved and taken over by the
  LizzieYzy Next import. The board candidates and the candidate table show the side to move, or
  Black with 总是显示黑胜率/目数 (the table's change column keeps Java's side-to-move difference); the graph,
  the statistics panel and the title always show Black. 盘面目数差 turns every shown lead into the
  board difference (plus the komi for Black, minus it for White) and switches the statistics
  label to 盘面. The 带评论 export follows the same settings.
- Analysis information (Java menus, keys and defaults; the settings are saved and taken over by
  the LizzieYzy Next import):
  - Win-rate graph (显示 > 胜率图设置): Black's win-rate line, the score line or both, 面积填充
    (two-sided around 50 % when one curve shows), 恶手损失条, 鼠标悬停信息 and 初始最大目数差.
    Gaps without analysis break the line, a branch shows the main line dashed from the fork,
    and the current move and hovered move carry their values. Click or drag navigates (a click
    on a branch may jump to the main line through the fork), the wheel steps through moves, and
    a right click clears a selected table row or steps back.
  - 统计 shows the score label, the bar of both sides' win rates and 最后一手 with the move's
    candidate rank and the change for the side that played it (against the position before, or
    two moves back when that one has no analysis). AI总评分 shows each side's mean match share
    over its analysed moves (a move counts as matching AI among the first 匹配AI候选数
    candidates with at least the given share of the most visits). Values the analysis has not
    produced are left out rather than shown as zero.
  - 选点列表 keeps the engine's order with the actual move marked or inserted (实战), Java's
    colours and resizable, remembered columns; a click shows a row's variation (the wheel over
    the list or the board lengthens it), a second click or a right click clears it.
    独立选点列表 (`U`) opens it in its own window with 7 or 9 columns (sortable by header),
    列表 / 柱状图 / 当前手 / 鼠标悬停, 总计算量 / 最高计算量 / 集中度, 总在最前 (`Q`) and a
    remembered position for each layout. It shows the side to move's values, as in Java.
  - Kata形势 (显示 > Kata形势显示, the bottom-bar button, `.` with 使用快捷键): KataGo is asked for
    the ownership while it is shown, drawn on the main board, the sub board, both or hidden, in
    the five square styles; 按选点显示评估 shows a hovered candidate's own ownership, 记忆评估显示状态
    remembers the boards for the next start.
  - 终局点目 (棋局 menu, `Ctrl+Q`, bottom bar when added): analysis pauses, clicks mark dead
    groups and 不计入 points, the 点目结果 window counts by area or territory (切换规则), 重新计算
    starts over, 确定 writes the result (`RE`, as `B+3.5` / `W+0.5`) into the game info, and
    leaving resumes analysis. Unlike Java, which put its localized wording (黑胜3.5目) into
    `RE`, the result is written in SGF form so other programs can read it.
  - 记录选点信息到评论中 (显示 > 主界面设置, on by default): while an engine runs, the comment pane
    shows the move's comment with its analysis summary, and saving writes those summaries into
    the move comments. Saving also writes the analysis itself (`LZ`, `LZOP`) and opening a record
    reads it back, so a reopened file shows its analysis without an engine.
- Board display (显示 menu, keys, bottom bar and the top 选点 黑 / 白 boxes, Java defaults; the
  settings are saved and taken over by the LizzieYzy Next import):
  - 坐标 (`C`): a smaller board margin when off; 坐标格式 (综合设置 > 对局与操作): normal, with I,
    野狐 (with I, rows from the top) or numbers from the top / bottom. 显示名字 puts the player
    names under the board.
  - 手数: 不显示 / last 1, 5, 10 / 全部 / 自定义, 总是从1开始显示, 分支内从1开始 or continuing
    the total, 分支内总是显示所有手数 (`Ctrl+M`), 分支面板上的手数; `M` and the bottom bar cycle
    全部 → 最近1手 → 不显示. Numbers are worked out by the core: a branch starts at 1 only if it
    was played (or the record was opened) while 分支内从1开始 was on, an SGF `MN` renumbers, captured
    stones lose their number, setup stones take none.
  - 落子评价 (`Alt+M`, toolbar button with its popup): marks of the last move, the last n or all
    moves, judged by score, win rate, both or automatically, from the analysis of the positions
    before and after each move; a move without that analysis gets no mark. They show while move
    numbers are off.
  - 推荐点: win rate / visits / score rows and their order (自定义信息顺序, with Java's preview),
    score as difference, order badges, red maxima, white text for White, no refresh under the
    mouse, the variation on hover (`Shift+F`), delayed candidates, replay interval (`R` replays
    the hovered variation step by step); `F` hides the candidates, holding `Z` hides them until
    released.
  - 综合设置 > 引擎与分析: blue ring on the best candidate, colour concentration, White's text
    and badges in white, circles for candidates beyond the limit (otherwise those are left out,
    except one that just dropped out, for 3 s), the low-visit ratio, PV visits on the hovered
    variation (last move or every move, above a threshold; KataGo is asked for `pvVisits`) and
    removing captured stones in variations.
  - 设置自动播放 (`Ctrl+A`, bottom bar 自动播放): main board (one move every n seconds, or the
    best move), sub board (its variation one move every n ms) and branch replay on hover (with
    the whole variation first for n seconds).
  - 下一手 (`J`: 详细 → 简易 → 不显示): 详细 shows the record's next move with its result and a
    good / questionable / bad colour once both positions are analysed (minimum visits setting);
    hovering it shows that move followed by the next position's best variation.
  - 策略网络 (`T`) shows the candidates' priors (no hover variation meanwhile); 纯网络 (`H`) asks
    the engine for its raw network
    (`kata-raw-nn`, or `heatmap` for Leela Zero) instead of analysing: the policy of every empty
    point and the raw win rate line; with KataGo a second `H` adds the ownership (纯网络形势: five
    styles), a third returns to the analysis. A raw result is only shown for the position it was
    asked for.
  - 小棋盘设置: the sub board shows its own candidate's variation (click / right click / wheel,
    paused while the pointer rests on it with 小棋盘不刷新), the first-instinct (prior) or
    after-calculation (visits) heat map, or the stones only. The variation tree shows each
    node's move number (分支面板上的手数) as the board numbers it.
  - Two changes from Java: a pass inside a branch keeps the branch's count, and setup stones
    right after the root no longer start the count at 2; 纯网络形势 > 小方块 has its own effect
    (Java read the KataGo-estimate setting there); 设置自动播放's 落子最佳选点 shows its own setting
    (Java filled it from the other check box).
- Rules: an SGF record's `RU` is applied to KataGo (`kata-set-rules`) before analysis. When the
  engine cannot confirm it, a dialog asks whether to continue with the engine's current rules
  (no pauses analysis). `Shift+D` opens the KataGo rules editor.
- GTP console (`E`) shows the engine traffic and sends commands. Like Java, `komi <v>` sets the
  game komi and `undo` steps back in the record; other commands that would change the engine's
  board (`play`, `clear_board`, `place_free_handicap`, …) are refused because the record owns the
  position. The 完整日志 option writes the GTP traffic to `logs/engine-trace.log`. A failing
  engine shows Java's failure dialog with its output, copy and diagnostics export. Engines are
  ended (`quit`, then killed after 3 s) when the application exits, including application Quit.
- Logs, the engine trace, failure details and exported diagnostics pass through a port of
  Java's credential sanitizer (`--password …`, `token=…`, JSON credential keys, authorization
  headers, URL user info, encoded forms); the live console shows raw traffic like Java.
- Importing LizzieYzy Next settings: offered at the first start (no settings of this client
  yet) and in Help > 从 LizzieYzy Next 导入设置…. Choose the Java settings folder (the one with
  `config.txt`; Java's per-user folders such as `~/.lizzieyzy-next` and, on Windows, the shared
  `LizzieYzyNext` folders are suggested when they exist). A preview lists every Java setting as
  imported, not usable here (invalid values; Java window positions) or left out (features not in
  this client yet, accounts and credentials), with the engine list. Imported: record settings
  (load komi, go to last move, replace prompt), language, stone sounds, the editing settings
  (double click, drag, right-click menu or back, new-board prompt, markup tools, 其他 board
  size, no-capture), recent records, last
  folder, the engine list with its default and start-up choice, each engine's thread policy and
  benchmark results and its GTP configuration profile, analysis limits and interval, fast engine
  change, limit notice, KataGo rules, the start-up benchmark choice, the network proxy and the
  board display settings (coordinates, move numbers including the older `show-move-number` keys,
  move-rank marks and judgement, candidate rows and options, next-move hints, sub-board mode,
  pure-network ownership style), the analysis-information settings above (graph, Kata形势,
  scoring rules, AI match, 独立选点列表 options, 记录选点信息到评论中; Java's window and column
  positions are reported as not usable) — only values Java itself accepts. Java's marker for its own
  bundled KataGo package is reported as not
  applicable. Engines keep using the Java engine and
  model files: relative paths are resolved as Java launches them (a bare program name is looked
  up on `PATH`; relative files, including a Java engine's `-jar` file, in the Java folder or an
  optional Java program folder), and missing programs or `-model`/`-config` files are listed
  without starting anything. Remote (SSH) engines are left out, and so is a default or last
  engine choice that points to a left-out engine; an engine whose command holds a credential is
  imported only after ticking "复制凭据", and credential values never appear in the preview,
  report or logs. Once this client has its own settings, every setting the import would change is
  listed and replaced only after confirming. Importing writes this client's `config.json` and
  `persist.json` together (a failed write leaves both unchanged; an existing settings file that
  cannot be read is not replaced), restarts the engines with the imported choice, and shows the
  report; the language applies after a restart. Java's files are never written, so both programs
  can be used side by side.
- An analysis time limit of 0 seconds means 9999 minutes, as in Java.
- Engine resources (Settings > 引擎一键设置, the toolbar 一键设置 button, the bottom bar 更新官方权重
  button for the weights page, and the "AI 未就绪 - 点击修复" banner shown while no engine has a
  command): this client does not bundle an engine. Opening the dialog detects the computer (OS,
  CPU architecture and threads, memory, NVIDIA GPUs through `nvidia-smi` with VRAM, driver and
  compute capability, Apple chip and Rosetta through `sysctl`, CUDA/cuDNN libraries and the OpenCL
  driver on Linux, `OpenCL.dll` on Windows); a value that cannot be read is shown as unknown. It
  then lists every KataGo package for this platform with its tier (stable, optional, experimental),
  source, license, runtime needs, size and capability, recommends one with its reasons and missing
  dependencies, and offers the three Java model choices (balanced, strongest (default),
  lightweight). Nothing is downloaded, installed, switched or replaced without a click and a
  confirmation; experimental packages (Windows DirectML, OpenVINO, ROCm) are never recommended and
  ask first. Downloads resume and can be stopped (the partial file is kept for a later resume).
  Engine packages and the three Java models are checked against pinned SHA-256 digests and sizes,
  the NVIDIA runtime against the digests in NVIDIA's release manifests (TensorRT and NVRTC are
  pinned), and the strongest network against the digest katagotraining.org reports; other weights
  from the online list are only checked against their download size (at least 1 MiB). A mismatch
  deletes the partial file and is reported. An engine package can also be imported from a zip
  obtained elsewhere (accepted only when it is the same trusted package), a weight file
  (`.bin.gz`/`.txt.gz`) can be imported,
  and an existing KataGo can be chosen. "验证" runs `katago version` (and, for NVIDIA drivers
  between 528.33 and 570.65, KataGo's one-second inference probe). "使用此配置…" shows the exact
  profile it will add (or update, for a profile this dialog created whose command is unchanged)
  and saves it only after confirming; profiles you configured yourself are never changed. On
  Windows the NVIDIA page installs the CUDA 12.8 / cuDNN 9 / TensorRT 10.9 runtime from NVIDIA
  after confirming (drivers are never installed). The official online weight list
  (katagotraining.org) can be refreshed; a failed refresh keeps the previous list. Selected-move
  focus is not supported by these releases; contribution is not provided or verified here.
  Network access follows Settings > 网络代理 (direct by default, system, or a manual HTTP proxy).
- Engine performance: MoreEngines has Java's thread policy (CFG uses the engine's config file;
  Benchmark uses the measured recommendation and sends `kata-set-param numSearchThreads` at start),
  性能测试 opening the performance page, 扫描 to build a command from chosen files (including the
  ikatago account form), and GTP 配置 for engines that publish a configuration schema (saved with
  the profile, the engine's config file is not edited). The official benchmark runs only when
  clicked, or after start-up when "首次启动时引导运行智能测速优化" is turned on (off by default);
  it can be stopped. Its recommended search threads stay in use until the policy is changed or
  measured again; the measured server threads and batch size apply only while the engine, model
  and config are unchanged, and the page then marks the result as historical. Measured tuning
  reports can be reviewed, applied and restored; they and Apple Silicon tuning apply only while
  the content of the engine, model and config still matches, which is checked before the engine
  starts without blocking the window. Apple Silicon deep tuning is experimental and asks first.

Every other Java menu item, toolbar button and shortcut is present in its Java place but marked
as not yet available (greyed in menus; a notice when used). Not available yet: comparison and
dual-engine analysis, remote/SSH and cloud engines, the quick-analysis and HumanSL models of the
weights page, whole-game measured tuning, 启用点击棋子回看 (click review), 自由涂鸦, and the
analysis-region items of the board context menu.

Resource acquisition and real engine analysis have been verified on Linux (private X11,
WebKitGTK), including downloads of the official KataGo v1.18.1 Eigen AVX2 AppImage and model
resources. Native integration runs have also exercised real CUDA analysis on Windows 11,
Metal analysis on macOS 27 / Apple Silicon, and CPU analysis on Ubuntu 24.04 GNOME Wayland,
with native Open/Save, SGF/GIB navigation, saved-record reopening and both window sizes.
Performance acceptance remains incomplete: macOS client working-set growth and GTK native-open
latency remain over budget. Some macOS 20-key navigation and GNOME small-window first-analysis
comparisons remain measurement-uncertain; these are not performance passes.

On Linux X11 with only VMware `vmwgfx` graphics cards, startup selects WebKit shared-memory
frame transport to avoid the observed DMA-BUF fence stall. GL rendering is retained; completed
frames are read back to shared memory, which has a copying cost. This does not select a software
renderer. Native Wayland, other graphics drivers and mixed-GPU systems keep their existing path.
Explicit graphics environment settings take precedence, including
`WEBKIT_DMABUF_RENDERER_FORCE_SHM=0` to retain WebKit's normal transport for diagnosis.
On Ubuntu 24.04 GNOME with Mesa 25.2.8 / VMware SVGA3D, the packaged compatibility route
has been exercised with both deb and AppImage: board display, keyboard focus, real analysis,
native Open/Save, record reopening and natural client/engine exit. The deb run covers both
window sizes; a native Wayland regression also passed without selecting SHM. This is functional
compatibility evidence, not a new performance pass or proof for other driver/WebKit versions.

The package-only predecessor was verified separately as follows:

Packages of both identities (below) were built for Windows x64, macOS Apple Silicon and Intel,
and Linux x64.

- Windows 11 x64: the acceptance-identity installer has been installed, upgraded in place and
  uninstalled, and the acceptance-identity portable zip extracted and started. The
  normal-identity installer has been installed next to the acceptance identity, both running at
  the same time, and uninstalled; the normal-identity portable zip extracted and started. All of
  them opened records from the command line, and both installed identities also through the
  native Open dialog.
- macOS 27 on an Apple Silicon Mac: the Apple Silicon apps of both identities started from their
  dmg and opened records; the acceptance identity also through the native Open panel. The Intel
  app has been started only under Rosetta. The Gatekeeper first-open steps in
  [docs/installing.md](docs/installing.md) were followed for a quarantined copy of the normal
  identity.
- Ubuntu 24.04 and 26.04 containers under a virtual X display: the deb packages of both
  identities have been installed side by side and removed; only the normal-identity deb was
  upgraded in place, on 24.04. The normal-identity AppImage has been run on Ubuntu 24.04.

Not verified yet: macOS 15, Intel Macs, Ubuntu 26.04 GNOME, Windows 10, the Windows SmartScreen
prompt, and full input-method coverage on Windows and macOS.

## Layout

| Path | Contents |
| --- | --- |
| `crates/lizzie-core` | Platform-independent core: game record model, SGF/GIB readers and SGF writer, browsing, settings, the LizzieYzy Next settings reader (`java_import`), the engine runtime (GTP framing, launch, analysis requests, rules, thread policy and benchmark), engine resources (`resources`: trusted package/model catalog, machine detection, recommendation, downloads, installs, verification), the credential sanitizer and the workbench command/view contract. No Tauri or DOM dependency. |
| `src-tauri` | Desktop shell: window, file dialogs, drag and drop, engine and helper processes, HTTP downloads, logging, diagnostics export, and the optional acceptance channel. |
| `ui` | Web UI: menus, toolbars, canvas painters, dialogs and the Java UI strings. |
| `fixtures` | Sample records used by tests and manual checks. |
| `tools` | Maintenance scripts. |

The core owns the open record. The UI only renders `WorkbenchView` projections and sends
`Command` values through one Tauri command (`dispatch_command`); menus, toolbar buttons,
shortcuts and automated acceptance all use the same commands. Views carry an increasing `seq`,
and an open runs in the background under a ticket so a late result from an older open is
dropped. Engine results carry the record, position and komi they were requested for; the core
drops any that no longer match and sends the engine only the moves that changed.

The desktop shell sends view updates through an ordered Tauri IPC channel. Engine-driven
updates are coalesced at 50 ms; user commands publish immediately. Startup subscribes before
taking a full snapshot, then discards older queued views by sequence number. Analysis and
history panels compare their fields to decide whether to repaint, including changes to a
variation or its captures when the visit count stays the same.

Record content changes only through the core's document edit operation, which raises the
document `revision`. A save captures the text, document and revision when it starts and is
written by one background writer in order; its receipt (`save_outcome`) confirms only that
revision of that document. `DocumentView` reports `revision`, `saved_revision` (the revision that
is the content of the document's file) and `unsaved`, so edits made while a save runs, or a
record opened meanwhile, stay unsaved. Every node carries a stamp that changes whenever its
position changes (or it is removed); stored analysis and the engine's synced position are keyed
by node and stamp, so an edit drops exactly the analysis of the positions it changed.

Edits that ask first (delete with following moves, new board, setup mode, starting-position
conversion) answer `NeedsEditConfirmation` with an `EditTarget` (document, revision, current
node); the confirmed command carries that target back and does nothing once any of them has
changed. Menus, toolbars, keys, context menus and automation all send the same `Edit` command.
Record readers take the no-capture setting when a read starts (`formats::read_file` /
`formats::parse`) and place each move as Java does while reading; each node keeps the rule it
was placed with, so switching the setting never changes existing positions.

## Development

Requirements: Rust (stable), Node.js 22+, and the [Tauri 2 system
prerequisites](https://v2.tauri.app/start/prerequisites/) for your platform.

```sh
npm install
npx tauri dev                       # run with hot reload
npx tauri build --no-bundle         # release binary in target/release
npm run typecheck                   # TypeScript
cargo test --workspace              # core tests (records, browsing, saving, engine protocol, contract)
cargo clippy --workspace --all-targets
```

### Generated files

- `ui/src/contract/generated.ts` is generated from the Rust types. After changing them run
  `UPDATE_CONTRACT=1 cargo test -p lizzie-core --test contract`; the plain test fails while the
  file is out of date.
- `ui/src/i18n/javaStrings.json` holds the Java UI strings used by the UI, taken from the Java
  `l10n/DisplayStrings_*.properties` bundles. Regenerate it with
  `python3 tools/extract_java_strings.py <java-checkout>`; the script requires the checkout to be
  at the reference commit and lists keys that are shown literally.

### Acceptance channel

Builds with `--features acceptance` (for example
`npx tauri build --debug --no-bundle --features acceptance`) open a loopback-only control
channel when `LIZZIEYZY_TAURI_ACCEPTANCE_PORT` is set. It takes newline-delimited JSON:
`{"command": <Command>}` runs a command exactly as the UI does, `{"query": "view"}` returns the
current view without consuming anything the UI receives, and `{"query": "ui"}` returns what the
window shows (title, focus, open dialogs, status lines, tree scroll, last stone sound and the
positions of visible controls). With the feature, `LIZZIEYZY_TAURI_RESOURCE_BASE=<url>` makes
engine resource downloads fetch `<url>/<file name>` instead of the trusted source (digests and
sizes stay pinned), for isolated download tests. The feature is off by default; builds made
without `--features acceptance` contain neither, whatever the profile.

### Packages

`node tools/package.mjs` builds the packages of the committed checkout for the machine it runs
on and writes them to `target/packages`, with a `.manifest.json` (source commit, toolchain,
sizes) and a `.sha256` file per platform. It refuses a working tree with changes unless
`--allow-dirty` is given, which the manifest then records.

| Platform | Build on | Command | Assets |
| --- | --- | --- | --- |
| Windows x64 | Windows with the MSVC build tools | `node tools/package.mjs` | `…_windows-x64-setup.exe` (NSIS), `…_windows-x64-portable.zip` |
| macOS Apple Silicon | a Mac | `node tools/package.mjs --target aarch64-apple-darwin` | `…_macos-arm64.dmg` |
| macOS Intel | a Mac | `node tools/package.mjs --target x86_64-apple-darwin` | `…_macos-x64.dmg` |
| Linux x64 | any Linux with Docker | `tools/packaging/build-linux.sh` | `…_linux-x64.deb`, `…_linux-x64.AppImage` |

Asset names are `<binary>_<version>_<os>-<arch><form>`, for example
`lizzieyzy-tauri_0.1.0_windows-x64-setup.exe`. Linux packages are built in an Ubuntu 24.04
container (`tools/packaging/linux-builder.Dockerfile`), because a binary built on a newer
system needs that system's glibc and does not start on Ubuntu 24.04; the script packages the
committed `HEAD` only.

`--identity acceptance` builds the same program as *LizzieYzy Tauri Acceptance*: identifier
`io.github.qiyi71w.lizzieyzy-tauri.acceptance`, binary `lizzieyzy-tauri-acceptance`, its own
install location, package name and data directories, and the acceptance channel compiled in
(`src-tauri/tauri.acceptance.conf.json`). It installs next to the normal program without
sharing anything with it. Packages of the normal identity never contain the channel.

The installers and the macOS app carry no paid code-signing certificate: the Windows files are
unsigned and the macOS app is ad-hoc signed and not notarized. See
[docs/installing.md](docs/installing.md) for what users see on first start.

## Data

Settings (`config.json`, `persist.json`) live in the platform configuration directory for the
identifier `io.github.qiyi71w.lizzieyzy-tauri`; logs (`logs/app.log`, `logs/crash.log`) live in
the platform data directory for the same identifier. On Linux these are
`~/.config/io.github.qiyi71w.lizzieyzy-tauri/` and
`~/.local/share/io.github.qiyi71w.lizzieyzy-tauri/logs/`; on Windows both are under
`%APPDATA%\io.github.qiyi71w.lizzieyzy-tauri\`. A failure before the window opens (for example
a missing WebView2 Runtime) is also written to `crash.log` and, on Windows, shown in a message
box. Downloaded and imported engine resources
live under `resources/` in the same data directory (`engines/<package>/` with a receipt,
`weights/`, `configs/gtp.cfg` and `configs/analysis.cfg` (created once from the package's example
files and never overwritten), `runtime/nvidia/` on Windows, `downloads/` for package archives; an
interrupted download stays next to its target as `<name>.part` and resumes from there); "打开程序目录"
opens it. LizzieYzy Next keeps its own
`config.txt` and `persist` elsewhere (for example `~/.lizzieyzy-next/`); this client never writes
there. The first start is detected by the missing `config.json`; declining the import offer
writes it.

### Engine resource sources

The trusted catalog (`crates/lizzie-core/resources/catalog.json`) pins qualified official
KataGo release assets separately for each platform and backend: v1.18.2 for CUDA and v1.18.1
for the other upstream packages. The macOS Metal packages are self-contained builds made by
this project from the official v1.18.2 tag (`.github/workflows/katago-macos.yml`,
`tools/katago-macos/build.sh`) and published as release assets of this repository; they carry
the licenses of KataGo, of the components vendored in its source and of every bundled library,
and the build stops when one is missing.
Each package records its own origin, source tag and commit, release, licenses, archive and
executable SHA-256 digests and sizes. Downloads and offline imports must match that identity;
installation checks the root executable and all required files and directories before activation.
Packages live in `engines/<id>-<archive-sha256-prefix>/`, so a new package does not replace one
an existing profile still references.

Official Linux executables are AppImages and require `/dev/fuse` plus `fusermount` or
`fusermount3` on PATH. Recommendations check each package's system libraries, the CUDA/cuDNN
library versions the system loader actually selects (read from its cache and soname links),
minimum NVIDIA driver and CPU AVX2/FMA requirements. Unknown facts remain
unknown and explicit verification reports runtime failures. Windows NVIDIA runtime components
come from NVIDIA's redistributable archives: CUDA 12.8, cuDNN 9.8.0, NVRTC 12.8.61 and TensorRT
10.9.0.34 are pinned, with license files kept next to the installed libraries.

Analysis is supported. Selected-move focus is not supported by these releases. Official
non-ONNX builds include upstream distributed support, but this app does not verify contribution
eligibility or provide a contribution flow here; DirectML and OpenVINO builds do not support it.
Models remain the b11 network from katagotraining.org (KataGo Neural Network License) and the
two b10 networks from the KataGo v1.17.1 release. License references are shown per asset; official
packages do not need the `licenses/` folder or `source-package.json` that LizzieYzy Next packages carry.

## License

GPL-3.0-only (see `LICENSE.txt`). The images, stone sounds and the winrate font in `ui/public` are copied
unchanged from LizzieYzy Next (GPL-3.0), and the application icons are generated from its icon;
see `ui/public/assets/SOURCE.txt` and `ui/public/fonts/SOURCE.txt`.
