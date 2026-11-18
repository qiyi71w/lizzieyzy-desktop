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
  界面 > 面板 > 分支面板设置 chooses how it fits: unticking 使用滚动面板拓展宽度 lays the tree out
  inside the panel without scroll bars, with the current node in the middle row and the wheel
  stepping through the moves as over the board (the other two items are then disabled).
  设置最大宽度 (10000 by default) cuts a scrolled tree at that width; a tree that reaches it is
  laid out inside the panel instead, or keeps scrolling with the part beyond the width hidden when
  超过宽度依然使用滚动条(超过部分将不显示) is ticked. That state lasts for the open record until
  another record opens or the maximum width is confirmed again. All three settings are saved and
  imported from LizzieYzy Next.
- While a candidate's variation shows on the board (a clicked candidate-list row, or the
  candidate under the pointer), the browsing keys step the variation instead of the record, as in
  Java: `Up`/`Down` and `PageUp`/`PageDown` show one move less/more; `Ctrl+Down` shows 10 more
  moves and `Ctrl+Shift+Down` as many more as the next comment is away; `Ctrl+Up` and
  `Ctrl+Shift+Up` show one move less. The board menu's 回退一手 and the control panel's buttons
  do the same (`>`/`>>` one/five more moves, `<`/`<<` one less). The record stays on its move;
  without a shown variation these entries browse as usual. The bottom bar buttons always browse.
  In 双引擎模式 `Ctrl+Down`, `Ctrl+Shift+Down` and the panel's `>`/`>>` grow the main board's
  variation only (the right board's when only it shows one); the other entries step both boards.
  A variation that auto-replays (自动回放变化图) is stepped from the length it shows, and the
  replay goes on from the new length. During an `R` replay (回放变化图) these keys and the wheel
  step from the length the replay shows and end the replay, as in Java.
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
  saves still being written and record the window placement. On macOS, `⌘Q` (Quit), `⌘H` and
  `⌥⌘H` (Hide) always belong to the native application menu, in every window, as in Java;
  other Command shortcuts act as Java's Ctrl, and unhandled ones remain available to the menu.
  On Linux, the saved size is the WebView content area, excluding GTK window decorations,
  so reopening does not enlarge the content area by the title bar or shadow dimensions.
  File > 强制退出 skips both the pending-save wait and window-placement capture; use normal
  close or Quit when a save must finish. Both exit paths end the whole application, including
  hidden tool windows such as 野狐棋谱 and 腾讯棋谱, and shut down the owned engines.
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
    extension decides the format and `.png` is added otherwise. Errors show Java's message. In
    浮动棋盘模式 the main board picture is the 浮动主棋盘 window; while 浮动小棋盘 shows, the sub
    board picture is that window.
  Exports never replace the open record's revision or saved state otherwise; a failure leaves
  the old file untouched.
- File > 复制主棋盘到剪贴板 (`Shift+C`), 复制小棋盘到剪贴板 (`Alt+C`) and 复制棋谱到剪贴板
  (`Ctrl+C`) put the picture or the SGF text on the system clipboard; the board pictures come
  from the floating windows as for the 截图 entries.
- File > 存档与读档 or the toolbar's 存档 button opens Java's save-and-load panel: slots with the
  board picture, name and time; 新建存档, 保存 (asks first), 改名, 删除, 读取 (or a click on the
  picture), 全部删除, and the check boxes 鼠标悬停放大缩略棋盘, 自动存档 and 启动时自动恢复. A loaded
  slot opens as a new unsaved record without a file of its own (saving asks for a name) and the
  title shows the slot. With 自动存档 on (the default) the automatic slot is written every
  60 seconds and when the window closes; File > 强制退出 writes none. If the application ended
  without its exit save, the next start keeps the newest periodic save as the automatic slot.
  In 浮动棋盘模式 the slot pictures show 浮动主棋盘. A slot keeps its picture only when the
  picture shows the position the record is saved at; a save whose picture cannot be taken, or
  whose position moved on meanwhile (stepping through moves in 浮动棋盘模式 while the automatic
  save runs), still writes the record, without a picture.
  With 启动时自动恢复 on, a start without a record to open resumes the automatic slot at the end
  of its main line. Slots live in this program's data folder (`save/`), never in Java's.
- 野狐棋谱 (同步 menu, the toolbar's 野狐棋谱 button or the bottom bar's 同步 popup) opens Java's
  search window; opening it also lets 启动Lizzie时预加载引擎 start the analysis process, as in Java.
  Search a Fox account by 昵称 or UID (搜索 or `Enter`); 当前野狐账号 and up to eight 近期搜索
  buttons (a saved UID searches by UID; 清空历史 asks first) show the account. The table lists 25
  public games per page with Java's columns, the account's wins in red; `|<` `<` `>` `>|` page
  through them, and on the last page (or the one before it, once there are four) the next Fox
  list is fetched, until 没有更多棋谱. 打开 or a double click downloads a game behind a progress
  overlay; it opens through the ordinary open path as a new unsaved record without a file of its
  own (saving asks for a name), and 打开棋谱后此窗口 then minimizes, hides or keeps the window. An
  unknown account, a network error or an unreadable game shows Java's message and keeps the
  current record. Closing the window or `Escape` hides it with its list. The window uses only
  Fox's public read-only queries, follows Settings > 网络代理, tries each request up to three
  times, and never writes the account or Fox's replies to the log. Downloaded SGF is corrected
  like Java (Fox's windowed variation tree is merged back) before the ordinary SGF reader reads
  it. The searched account, 查询方式, 打开棋谱后此窗口 and 近期搜索 are kept in the settings and
  imported from LizzieYzy Next; Help > 清除所有个人数据 also clears 近期搜索. Unlike Java, a failed
  list page does not end a game download that is running. Fox's list currently ignores its paging
  cursor, so the next list repeats the newest games (Java sends the same request).
- 腾讯棋谱 (同步 menu or the toolbar's 腾讯棋谱 button) opens Java's Tencent Weiqi search window,
  laid out like 野狐棋谱; opening it also lets 启动Lizzie时预加载引擎 start the analysis process.
  Enter a Tencent Weiqi nickname or numeric UID (搜索 or `Enter`; an all-digit text is also sent
  as the UID); 当前腾讯查询 and up to eight 近期搜索 buttons (清空历史 asks first) show the query.
  The table lists 25 games per page with Java's columns (time, players with their UIDs, ranks,
  result, moves), the searched player's wins in red, and the last pages fetch the next Tencent
  list behind a 正在加载更多腾讯棋谱 card until 没有更多棋谱. 打开 or a double click downloads a
  game; it opens through the ordinary open path as a new unsaved record without a file of its
  own, with every variation and comment the service sends. As in Java, the record's KM is not
  taken as the komi (Tencent writes komi × 100) unless the game starts from setup or handicap,
  so the current komi stays. 打开棋谱后此窗口 minimizes, hides or keeps the window; until chosen
  here it is 野狐棋谱's choice at the time the window is first opened in this run. A network
  error, a service error or an unreadable game shows Java's message and keeps the current
  record; an empty record reply or a successful reply without a list or record is reported as a
  failure instead of leaving the window waiting as Java did, and a failed list
  can be asked for again. The window uses only Tencent's public read-only queries, follows
  Settings > 网络代理, tries each request up to three times and never logs the query or the
  replies. The last query, 打开棋谱后此窗口 and 近期搜索 are kept in the settings and imported from
  LizzieYzy Next; as in Java, 清除所有个人数据 does not clear them. The service currently answers
  nicknames (including a player's English name) but returns no games for a numeric UID, and
  shows a resignation as a 0-point win (Java shows the same).
- Editing, at the Java entries (Edit and Game menus, the top edit and markup buttons, the bottom
  bar, board clicks, the two board context menus and the Java keys):
  - A board click plays the side to play (交替落子) or, with 添加黑子 / 添加白子, that colour
    (right click the other one). Clicking the point of the next main-line move follows it, an
    equal existing variation is followed, otherwise a new last variation is added. Occupied
    points, single-stone suicide, multi-stone suicide unless the foreground engine's rules allow
    it, and an immediate ko recapture are ignored without changing anything. `P` passes; `,` plays
    the best candidate (or the previewed variation); a middle click plays the previewed variation;
    自动补完征子 continues a ladder. A pass in the previewed variation is played too, so the moves
    after it keep the colours the board shows (Java's middle click and `,` skipped the pass and
    gave the next move the wrong colour).
  - 插入黑子 / 插入白子 (Edit menu toggle or the empty-point context menu) put a move after the
    current one; 反色, 删除 and 移动 on a stone (stone context menu, or dragging with 启用拖动棋子)
    change it where it was played. What followed is played again and keeps its comments, markup,
    setup stones and other properties; stored analysis of the changed positions is dropped.
  - `Delete`/`Backspace` 删除一手 and `Shift+Delete` 删除分支 ask first (Java's "删除" dialog)
    when moves follow; the bottom bar 删除 does not ask. An answer applies only to the record,
    content version and move it was asked for. 撤销删除 / 恢复删除 (shown only when
    possible) and the context menu 撤销编辑 / 恢复编辑 / 清除编辑缓存 undo and redo the last
    delete, insert or stone edit. `Shift+Left`/`Shift+Right` move the current branch, `L` and
    设为主分支 make it the main line, 添加为分支 adds a previewed variation beside the line, its
    passes included (Java ended the branch at a pass). A move without `PL` whose new first
    continuation is a move plays on in that move's colour, as the record reads after saving and
    reopening (Java kept the old side to play until the record was reopened); the move's stored
    analysis is dropped when its side changes.
  - 显示落子矩形 (综合设置 > 对局与操作 > 鼠标交互与坐标格式; 是 / 仅对局时 (default) / 否): a
    small square in the colour to play marks the empty point under the pointer on the main board
    and 浮动主棋盘. 仅对局时 means a game against the engine (人机对局 or 分析模式对局) on your
    turn; 终局点目 never shows it, and it goes when the pointer leaves the board.
  - 回看 a stone: with 启用点击棋子回看 (Edit menu or 综合设置 > 对局与操作, off by default), or
    from the stone context menu's 回看 whatever that setting says, the board shows the position
    before that stone was played (the latest such move on the current line) and the stored best
    variation of that position, one more move every 分支回放间隔. Moving to another point
    (after 300 ms), leaving the board or moving in the record ends it, as in Java; releasing the
    button alone does not. It only changes what is drawn: the current node, the record's saved
    state and the analysis stay as they are.
  - Comments: click the comment pane to edit; the text is saved to the node it was opened on when
    another place is pressed or the position changes. While typing, keys never play or navigate.
  - Markup (标记工具, shown when switched on): letters and numbers count on (A, B, … / 1, 2, …),
    triangle, square, cross, the eraser and 清除标记.
  - 自由涂鸦 (the top toolbar button after 清除标记, shown with the markup tools; also found by
    function search): a layer over the workbench between the toolbars to draw on while
    explaining. Drag with the left button to draw 3 px lines; the buttons at its top right choose
    the blue, green or red pen (the chosen one framed), take back the last line, clear every line
    and close the layer, as `Esc` does. While it is open the board and panels take no clicks,
    right clicks, wheel or keys. The lines are never saved: they do not enter the record or mark
    it unsaved, and they go when the layer closes. Only the pen is kept (saved on closing, used
    the next time, imported from LizzieYzy Next).
  - 起始局面设置 (Game menu): setup mode is available on a record without moves (otherwise it
    asks to turn the current position into the starting position) while no foreground engine
    runs, and no engine starts while it is on. Clicks place black or white setup stones or erase
    them (right click the other colour); 黑先 / 白先 set the side to play (`PL`). Setup stones take
    no move number, so the bottom bar's 手数 jump lands on the node that shows the number asked
    for, also past setup snapshots within the record (Java counted nodes and landed a move short
    there). 将当前盘面设为起始局面 converts the current position.
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
  (启用双击找子, 启用拖动棋子, 启用点击棋子回看, 显示落子矩形, 显示清空棋盘提示对话框, 标记工具,
  五子棋无提子规则). A setting that cannot be saved is reported and left unchanged.
- Help > 诊断与日志 shows this client's logs, opens the log folder and exports a diagnostics zip.
  诊断记录 and its four modules (引擎, GTP 摘要, 读盘/弈客, 网络/远程) take effect on 应用 without a
  restart and are saved (config `logging`, all on by default); they decide which summary records
  reach `app.log`. A failed network request is always recorded. 完整日志 is a session, not a
  setting: after a confirmation it writes the chosen scopes (引擎/GTP → `logs/engine-trace.log`,
  读盘/弈客 → `logs/readboard-trace.log`, 网络/WebSocket → `logs/network-trace.log`; none chosen
  means all) until it is stopped (here or with Help > 停止完整日志). The scope choice is saved for
  the next session; while one runs its scopes are locked, and a restart never resumes it.
  Like Java, each full log rolls over at 10 MB or on a new day into `logs/archive/`
  (`<name>.<date>.<n>.log.gz`), keeping 7 days and at most 100 MB of archives per log.
  Diagnostics exports include the active files.
- Help > 清除所有个人数据 clears the recent-record list and 野狐棋谱's 近期搜索; View > 重置界面位置
  resets the saved window placement and last folder and turns the overwrite prompt back on. Folders
  a Java settings import brings over afterwards, in one import or several, are kept for the next
  start; the reset placement does not come back.
- 搜索功能 (the top toolbar button, or `Ctrl+K`, `⌘K` on macOS, with no other modifier) opens a
  search bar centred over the window; it is refused while a text field has the focus (the key
  stays with the field and its input method) and while a dialog, menu or context menu is open.
  The bar stays in place and the results unfold below it: typing searches the Java function
  catalog in every interface language, pinyin and aliases included; 浏览功能, `Up` or `Down` with
  an empty query lists the whole catalog, which the category box narrows. A typed query lists
  its best 50 results and counts all of them. Each result shows its title and Java path; the
  selected one adds what it does, its shortcut, a prerequisite note and, when it cannot be used
  now, why. A click selects, a double click or 打开 or `Enter` (acted on when released, never
  while the input method is composing or on the key that commits its text) opens it, and `Esc`
  or 取消 closes the bar and gives the focus back to the control that had it, unless another
  control was chosen meanwhile. Opening a result does what the Java search does: a setting opens
  综合设置 at its section and focuses its row (2.4 s gold outline) without changing it; a
  navigation or setting entry on a menu or toolbar opens its menu path or shows its toolbar and
  focuses that control with the gold outline, without running it (a markup button also shows the
  标记工具 buttons for this session, as Java does, without changing the saved 标记工具 choice;
  `Space` then clicks a focused toolbar control); a 同步 popup item opens the 同步 popup with the
  item selected, also while 同步 is switched off or sits in 更多; a board context entry explains
  where to right-click and returns to the board; an action or window entry runs, as clicking its
  menu item or button does (上一手 takes back a move during a game like the bottom bar's `<`).
  Entries this client does not provide yet, or that the current state refuses (no engine, no
  KataGo, no edit to restore, a whole-game analysis running, …), are greyed with their reason
  and cannot be opened. `Ctrl+K` from the 浮动主棋盘 window gives the main window the keyboard for
  the bar; `Esc` or 取消 then gives the keyboard back to the 浮动主棋盘 window. While the bar or
  any other dialog is open, the workbench shortcuts and the floating
  boards' mouse input are inactive. Below a 900 px wide window the toolbar button shows its
  icon only.
- Engines: the engine list (更多引擎 / `Alt+X`) adds, edits, orders and removes engine profiles
  with Java's fields (name, command with Java quoting, initial commands, board size, komi,
  default engine, preload). The engine at start-up is the default one, the last one, none, or
  chosen from the list. `Ctrl+1`…`Ctrl+0` switch the foreground engine; with fast engine change
  the previous engine keeps running for a quick switch back. Analysis menu items close or
  restart engines. As in Java, an engine's board size applies to a new empty game, and its komi
  becomes the game komi unless the komi was set by hand (an engine that fails to start changes
  nothing). While an engine starts the status area shows 引擎正在加载中; after a failure it shows
  引擎未成功加载. As in Java, an engine that ended and was not started again is still the current
  engine: 重启当前引擎 starts it again. During an engine game the list's changes are refused with a
  message and the list keeps the saved engines.
- 自动检测引擎是否存活 (综合设置 > 高级 > 日志与分析刷新, on by default, Java's
  `ui.auto-check-engine-alive`): when the foreground engine's process ends unexpectedly after it
  was ready, or a remote engine's command ends on the host or its connection is lost, the same
  engine starts again in the foreground. It is brought to the current position with the game's
  komi, rules and board size (the profile's komi and board size are not applied again), and it
  analyses again unless analysis is paused; a position the user moved to meanwhile is the one
  synced. Java's non-modal 引擎异常关闭 window still reports the end, and the workbench stays
  usable. There is no restart when the setting is off, when the engine failed while starting,
  when it was closed, switched or ended by the user or at exit, when it was not the foreground
  engine, while it was lent to a background analysis (or being given back), or while it was left
  unrestored. An engine restarted this way is restarted again only after it produced an analysis
  line or a legal move, so an engine that keeps crashing is not restarted in a loop; a launch
  failure is reported like any start, and an engine that refuses the position it is brought to,
  or whose rules prompt for a record's rules it cannot apply is answered with No, is left
  unrestored (引擎状态未恢复 in the status area, no dialog, no further attempt). The
  ending is noticed at once from the process or connection, not by polling. Unlike Java, a local
  engine is restarted too (Java's check missed a local process that had ended).
- 棋盘同步工具 (同步 menu and `Alt+O`, Windows only): starts the bundled ReadBoard helper
  (`readboard/readboard.exe` next to the program) with Java's arguments; opening it again closes
  the running helper first. Boards the helper reads become the record's moves (a jump or a
  changed position becomes a snapshot of the synced board); with two-way sync a move played here
  is clicked on the target window, and the next move waits until the helper confirms it. The
  float board follows the helper's target rectangle and draws like Java's: candidates, the
  hovered variation (only its own stones over the target's board, captured points as
  board-coloured discs with a crosshair, step-by-step with 自动重放变化图, the wheel setting the
  depth; without auto-replay a variation shortened to one move, as one with too few playouts or
  beyond 最大候选点数 starts, stays hidden until the wheel lengthens it), and the 落子评价 and
  last-move marks only while move numbers are off. Unlike Java's float board, which shades
  落子评价 marks continuously, they use the main board's six levels and colours, so a move looks
  the same on both. Output of a helper that was replaced or has ended is ignored, and exit stops
  only the helpers this program started.
  棋盘同步工具选项 keeps Java's 总是保持棋盘一致 and 滚轮控制变化图时自动获取焦点; 同步时不播放落子声
  mutes stone sounds while syncing.
- GMA (the helper's engine auto-play; KataGo with `kata-genmove_analyze` only, otherwise 该模式仅
  支持 KataGo): each move reads the engine's pondering, time and visit parameters, sets the
  helper's limits, asks the move for the synced position, plays it and has the helper click it.
  While GMA holds the engine, 人机对弈, engine matches and HumanSL games are refused; while a move
  is in flight, switching, restarting or closing the engine (also through 远程算力 or applying a
  settings import), 跑谱贡献 and a Web 旁观 viewer's 试下 are refused too, and an engine that ends
  meanwhile is not restarted automatically. GMA is not armed while a 试下 holds the engine.
  Clearing GMA (stopping the sync or the helper ending) gives the original parameters back once
  the engine holds the record's position again; when a 试下 started between two moves holds the
  engine, they go back as soon as it ends. If the engine refuses them it stays unrestored
  (引擎状态未恢复) until it is switched or restarted. Arming GMA leaves a running analysis alone,
  as Java does: it goes on while GMA waits for the opponent, and each move's first engine command
  ends it before the move is asked. After a played move asked with pondering the engine goes on
  pondering by itself on that position; any other answer leaves the analysis paused (Space or
  the helper resumes it). Unticking 后台分析 in the helper
  (`playpon off`) while GMA is armed and no move is in flight, before its first move as between
  two, turns the engine's pondering off at once (its own value goes back when GMA is cleared) and
  stops a running analysis, as Java does; a 批量分析 started meanwhile stays paused.
  The 自建算力 engine (远程算力's WebSocket link) cannot ponder in the background: GMA leaves the
  pondering parameter alone on it, and when the helper asks for pondering (`playpon on`) each
  arming first shows ReadBoard 后台思考能力差异. GMA waits for the answer: 确定 or 不再提示 lets it
  play with the helper's fixed time and visit limits, 不再提示 is saved; closing the notice leaves
  GMA waiting until the helper arms it again.
- Analysis: candidates on the board (win rate, visits, score lead), the variation shown when
  hovering a candidate (captures are worked out by the core), the sub board cycling through
  candidates on click, the candidate tables, the win-rate graph, 统计 and Kata形势 (below), and
  the status and title lines. Candidates use
  Java's wood-backed allocation colours and row layout, including the separate visits font.
  Next-move dashed hints sit behind the labels; hovering a variation hides those hints until
  the pointer leaves. The bottom-bar analysis button uses pause/play
  icons and keeps its width when switching states. `Space` pauses
  and resumes for this session only (never saved); opening another record ends the pause (a
  paused 批量分析 stays paused, the pause being the batch record's). Time
  and visit limits stop the current position once and resume on the next one. Raising or
  unticking a limit (on the top toolbar or in 综合设置) lets a search a limit stopped go on at
  once when its visits and the time since it began are within the new limits; the resumed search
  stops again at the new limit. A stop by 空棋盘停止计算 is not lifted by a limit change. Results
  stay with the position, komi and rules they were requested for, so browsing quickly, switching
  records, changing komi or rules never shows another context's analysis; changing komi or rules
  discards the stored analysis like Java.
- 分析 黑 / 白 (top toolbar, shown with 自定义内容 > 选点分析控制): which sides analysis goes on for.
  It is a session choice, both ticked at every start and never saved. As in Java it is judged
  after each of your moves (including 停一手) and each browsing step or jump (forward, back,
  the variation tree, 跳转): when the side to move is unticked the engine stops there, without
  pausing; the next move or step to a ticked side resumes, and so does `Space`. Ticking or
  unticking the side to move starts or stops the current analysis at once. Opening or creating a
  record, 棋盘同步 and other whole-board syncs are not judged, and games, engine matches, AI 陪练
  and 自动分析 are not affected.
- Stored analysis and the Lizzie cache (分析 menu; 综合设置 > 高级 > 高级与性能, saved and taken over
  by the LizzieYzy Next import):
  - 启用 Lizzie 缓存 (on by default): a position's stored result is replaced only by a stronger
    one (more visits), after it was marked by 清除Lizzie缓存, or when it was made with another
    PDA than the engine now uses; automatic analysis, the playing engine of an engine match and
    总是覆盖 always replace. Off, every new result replaces the stored one. A live result is saved
    with the engine's PDA when it is nonzero.
  - 清除Lizzie缓存(全局) (`Shift+A`) / (此手) mark every analysed position or the current one: the
    analysis stays shown and saved, and the next result replaces it even when weaker. Nothing is
    sent to the engine.
  - 清除分析信息(全局) / (此手) remove the analysis of every position or the current one from the
    board, tables, graph and saved file (an erased position writes no `LZ` analysis, where Java
    still wrote a 50 % / 0-visit header). Then the current position is refreshed: the engine gets
    `clear_cache` if it lists that command, and a running analysis starts a new search. In 双引擎模式
    both engines' stored analysis is cleared and both engines are refreshed.
  - 空棋盘停止计算 (off by default): an ordinary analysis of the empty board (the root without
    setup stones) stops at its first result, without the limit notice; resuming it with `Space`
    searches on until the position changes. Games, engine matches and 自动分析 are not stopped.
- KataGo search parameters (standard KataGo in the foreground; Leela Zero and Sai have the options
  below, `KataGoPda` builds their own parameters):
  - Top toolbar 激进度 (`playoutDoublingAdvantage`) and 广度 (`analysisWideRootNoise`): ticking the
    box or each edit of the value sends `kata-set-param` at once and the analysis searches again;
    unticking sends 0. A value outside -3..3 (激进度) or 0..2 (广度) only turns the field red and is
    still sent, like Java; text that is not a number sends 0. A 广度 change lets every stored
    result give way to the next one. After an engine starts, both values are read back with
    `kata-get-param` and the toolbar shows what the engine holds (it keeps the user's own
    entries otherwise). The groups show only while the foreground engine is KataGo and no
    engine match runs. In a 人机对弈 game both reach the playing engine, except 广度 in a Genmove
    game. When an analysis game turned 广度 off (对局时不使用广度) and the user changed it during
    the game, the end gives back the 广度 field's value and ticks it again if 广度 (or its
    自动加载) was on before the game, else the engine keeps the last value sent. They
    are disabled while the engine is busy elsewhere (GMA, an unrestored engine, and for 广度
    野狐 AI 直连, whose session owns the 激进度 edits; its 激进度 stays shown while it runs).
  - 设置 > 引擎参数 (`Alt+D`) or the toolbar's 参数 opens 设置KataGo引擎高级参数 (without a loaded
    engine: Java's "no engine" hint). 确定 sends each row's value (0 when 修改 is off) and saves
    it; 自动加载 makes every KataGo that starts get that value before the read-back (the 激进度
    row is skipped for a `KataGoPda` engine); 在工具栏中控制 shows or hides the toolbar group.
    Without 自动加载 a starting engine keeps its own configuration values. The eight Java keys of
    these controls are taken over by the LizzieYzy Next import. Function search shows a hidden
    group, and a hidden top toolbar, for the current run without changing the saved choices.
  - 搜索线程数（本次运行） in the same window belongs to the running KataGo process only and is never
    saved. Opening the window shows the last value this process confirmed (读取中／刷新中) and reads
    `kata-get-param numSearchThreads`; a failed read shows 刷新失败. Ticked with a whole number
    1–1024, 确定 sends `kata-set-param numSearchThreads N` (确定, 取消 and closing wait for the
    answer) and saves the other rows once the engine accepts it, as they are then (a row edited
    while waiting is saved and sent as edited); an invalid number or a refusal
    (with the engine's reason) leaves the window open. Unticking an override of a local engine asks
    whether to restore the startup source (KataGo configuration file, saved benchmark
    recommendation with its value, or explicit launch command); confirming restarts the engine,
    brings it back to the current position, reads the new process's value and saves the other
    rows, and only then analyses again (unless analysis was paused). An SSH / 远程算力 / `ssh`
    command engine cannot be restored from here and
    keeps its override ticked; its state is unknown until set. A restarted engine, or another
    engine process, starts without the override, and the window reports a stale engine if the engine changed while
    it was open. Requests are refused while the engine is occupied (games, matches, GMA, a lease,
    an unrestored engine, a restart in progress; AI 陪练 searches with its own engine and does not
    count) and fail after 8 s without an answer.
- Leela Zero / Sai options: with Leela Zero or Sai in the foreground, 设置 > 引擎参数 (`Alt+D`) or the
  toolbar's 参数 opens 设置Leela(Sai)引擎高级参数 with Java's four rows (最大内存, 访问量, 延迟缓冲,
  认输胜率; the labels give Java's ranges, which are not checked). 确定 saves the rows and sends
  `lz-setoption name <option> value <value>` for each row whose 修改 is ticked, then the analysis
  searches again; an empty value sends nothing. A row with 修改 and 自动加载 is sent to every Leela
  Zero / Sai that starts. In an analysis-mode engine match, 访问量's 自动加载 sends
  `lz-setoption name Visits value 1000000000` to the Leela Zero / Sai participants before the
  game; when the game ends, a participant that keeps running gets back the 访问量 it was last
  given (自动加载 at its start or 确定 here), confirmed before it is handed back (Java left the
  1000000000 in place). The rows are global, as in Java, and the twelve Java keys are taken over
  by the LizzieYzy Next import. Other engines never get `lz-setoption`: for an engine that is neither
  KataGo nor Leela Zero / Sai (Sayuri, Pachi, Leela 0.11, unknown engines) the entry shows Java's
  "not Leela or Sai" notice instead of the window, where Java would open it.
- KataGoPda settings: the `...` button beside the toolbar's PDA controls opens the dedicated
  dialog. During an engine game (playing or paused) with a `KataGoPda` participant the button
  stays, whichever engine is in front, and the dialog's commands go to that participant.
  Choose 不启用PDA, 动态PDA with a nonzero coefficient, or 固定PDA with a value.
  确定 saves the choice; 取消 leaves it unchanged. An empty value is saved without sending a
  command. The dialog can also be used with standard KataGo, but only an engine whose GTP name
  begins with `KataGoPda` receives `pda`, `dympdacap`, `getpda` or `getdympdacap`.
  自动加载 applies the saved choice after startup read-back and again on the first white move
  of a handicap game. These are the Java implementation's triggers, not every new game. Like
  Java's saved command, the automatic apply of 动态PDA sends only `dympdacap <coefficient>`, so
  the engine keeps (and the toolbar shows) its own PDA; only 确定 also sends `pda 0`.
  The six Java PDA-dialog settings import as a mode, numeric values and an automatic-apply
  choice; an invalid `auto-pda` command is reported as an invalid value, never stored as a raw
  command. Explicit mode/value keys take precedence; a parsed command supplies missing fields.
- With a `KataGoCustom` or `KataGoPda` engine, right-click an empty intersection for
  优先分析此点 (`setmaxpolicy <coordinate> 1.1`) or 清除优先分析 (`clearpolicy`). Choosing a priority
  point starts analysis if it is paused. Reported priority candidates remain visible beyond the
  candidate-count and low-visit display limits, and a result that came while the point was chosen
  keeps it as a candidate for 棋力评估, as Java does. A move clears the temporary priority selection
  without sending `clearpolicy`; it is not saved in the record or preferences.
  The private protocol is covered with controlled engines; acceptance by a real KataGoPda
  binary remains unverified.
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
    numbers are off. 原棋盘不显示评价标记(棋盘同步时) hides them on the 棋盘同步工具 float board only.
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
- Workbench panels (界面 > 面板, Java shortcuts): 小棋盘 (`Alt+Z`), 胜率图 (`Alt+W`), 评论/问题手面板
  (`Alt+T`; disabled in 四方图 and 双引擎模式), 分支面板 (`Shift+G`), 选点列表面板 (`Alt+G`), 信息面板 and 状态面板
  show or hide their panels and are remembered; 放大小棋盘 (`Ctrl+F`) and 放大胜率图 (`Ctrl+W`) use
  Java's enlarged layouts (the large win-rate graph hides the list and, with the variation tree
  shown, the sub board until it is left again), 总在最前 (`Ctrl+Z`) keeps the main window on top.
  On macOS `Command` also works for `Ctrl+W` / `Ctrl+F`; `Ctrl+Z` takes the Control key only, as in
  Java. The panels sit where Java's fixed in-frame layout puts them for
  each combination. Dragging the board's left or right edge moves the board, dragging the
  comment's top edge changes its height (at least 68 px) and dragging the line between the
  variation tree and the list shares that column (the list keeps at least 36 px); a drag is
  saved when the button is released and `Esc` cancels it. `[` / `]` (界面 > 主棋盘位置) move the
  board one eighth of the free width; 面板 > 恢复默认面板尺寸 restores the default sizes. Holding
  `X` shows Java's key list over the workbench until the key is released; moving over the board,
  the variation tree or the list returns the keyboard to the workbench unless a comment is being
  edited. 工具栏 > 顶部工具栏 is 分开 (the second toolbar row), 合并 or 不显示 (no toolbar row, as in
  the fixed Java build). Its 自定义内容 items show or hide each group of the row, as in Java:
  独立/合并工具栏按钮 (off by default), 基本按钮, 落子工具, 强制计算工具, 对局控制, 选点显示控制, 规则,
  参数, 棋盘 and 存档 (on by default), and 单步分析限制(时间)(秒), 单步分析限制(计算量), 选点分析控制 and
  选点信息控制 (off by default). Each choice is saved at once and taken over by the LizzieYzy Next
  import; 对局控制 still follows the game state while shown, and 死活 has its own item. 独立/合并工具栏按钮
  puts a ▲ button first in the row that switches it to 合并 (the same setting as the menu).
  自动换行(仅独立显示时), on by default, lets the row wrap; off, it stays one line and what does not
  fit the window is cut off, with no scroll bar. 棋盘 opens 设置棋盘大小. The optional groups add the
  限制: 时间 / 计算 fields, 分析: 黑 / 白 and 选点 胜率 / 计算量 / 目数 to the row. The limit
  fields write 综合设置's analysis limits on every edit (digits only; an empty field keeps the
  value); the 选点 boxes are 显示 > 推荐点's win rate / visits / score items. Function search shows
  a hidden group for the current run without saving it. 底部工具栏 is 显示, 不显示 or (Chinese
  interface) 使用详细工具栏, which adds
  Java's 自动分析, 自动播放, 自动落子 and 引擎对战 panels under the bar. Its 自定义内容 items show or
  hide each bottom-bar button, as in Java: 同步, Kata形势 (with a KataGo engine), 自动分析, 纯网络,
  分析, 设为主分支, 清空棋盘, 删除, 手数, 坐标 and 自动播放 (on by default), 打开, 保存, 终局点目,
  选点列表, 刷新, 试下, 返回主分支 and 落子评价 (off by default), and 超级鹰眼 (off by default only when
  the top toolbar and its second row were both shown at start). Each choice is saved at once and
  taken over by the LizzieYzy Next import. The buttons keep Java's places; those that do not fit
  go to 更多, whose popup runs them. 打开 / 保存 act as 文件 > 打开 / 保存 (保存 overwrites the
  record's own file, otherwise asks for a name), 选点列表 as `U`, 落子评价 as `Alt+M`, 终局点目 as
  `Ctrl+Q`, and 刷新 lets the current move's stored result give way and searches the move again
  while analysing. In the Chinese interface, 详细工具栏(过时的) > 显示拓展/隐藏详细工具栏按钮 adds a
  button at the left of the bar that opens or closes the detailed panels without asking, and
  设置模块顺序 places the three panels (1–3, saved with the window placement and taken over from
  Java's `toolbar-parameter`). Function search locates a bottom button that is switched off or
  does not fit in the 更多 popup, without changing the saved choice, and opens the detailed
  toolbar for its controls. `Shift+X` and 设置 > 综合设置
  open 综合设置 at 棋盘与显示, whose 启动时加载 rows
  (窗口总在最前, 显示快速启动, 显示状态面板, 显示小棋盘, 窗口标题显示胜率) apply with 保存设置. Panel
  sizes, the bottom toolbar mode and the panel order are window placement (`persist.json`),
  cleared by 重置界面位置. While 综合设置 is open, however it was opened, no engine analyses; when
  the dialog closes (保存设置 or 取消) analysis goes on as before: a game, an engine match or
  自动分析 continues, and analysis the user paused stays paused.
- 快速启动 (shown in the menu bar while 综合设置 > 显示快速启动 is on; off by default): one item
  per saved outside program, by name, then 设置. Choosing a program starts its command at once,
  without asking or waiting; the command is split and its relative paths and working directory
  are resolved like an engine command. A program that cannot be started shows 运行失败!. 设置 opens
  快速启动设置: click a row of the 30-row table to edit it (a row past the list adds a program),
  type 名称 and 命令行 or pick a file with 自动生成 (on Windows only `.jar`, `.exe`, `.bat`), and
  保存; 删除 removes it, 上移 / 下移 / 置顶 reorder. Every change is saved at once, empty commands
  are dropped, and nothing is started while editing, importing or starting the app.
- 帮助 > 关于 opens 综合设置 at 关于 (also reachable from its navigation and function search):
  the product name, the running version (the same as in the window title), a note that this
  client is migrated from LizzieYzy Next (Java), and 项目主页 / 发布下载 / 问题反馈 buttons that open
  this project's GitHub repository, Releases and Issues pages in the system browser. Java's feature
  cards, QQ group and 查看更新 button are not shown.
- 设置 > 初始化设置 opens Java's quick settings form, filled from the current settings: 变化图 and
  小棋盘 refresh while hovered (不断刷新 / 不刷新), 目数 on candidates (盘面目数差 / 计算贴目后目数差), the
  胜率(目数) perspective (总是黑棋 / 黑白交替), the Lizzie cache, the 选点信息 items with 自定义显示顺序,
  限制选点个数 and 限制变化图长度, the per-move time and visit limits with their switches, and the
  language. 加载默认设置 puts Java's defaults into the form only (the language stays; an empty
  计算量 field keeps the saved visit limit); 确定 saves every row at once, through the same settings
  as the menus, toolbar and 综合设置; Escape or the close box saves nothing. While a mode refuses
  one of the changed rows (for example the analysis limits while 野狐 AI 直连 holds the engine),
  确定 shows that mode's notice, saves nothing and keeps the dialog open with the edits; Java's
  dialog has no such check and saved every row. A field that is not a whole
  number shows 错误的输入格式 and keeps the dialog open (a time or visit field whose switch is off is
  not checked); a time limit of 0 or less typed into the form is saved as 9999 minutes (a saved 0
  left as shown stays 0, which means the same; Java rewrote it as 9999 minutes on every 确定), and
  negative candidate / variation limits as 0 (no limit). The language applies from the next start,
  without a restart notice, as in
  Java. Java's 界面样式 row (Swing look and feel) has no counterpart here, and the form does not open
  by itself on first start.
- Appearance (Java defaults; saved and taken over by the LizzieYzy Next import):
  - 设置 > 主题 opens 综合设置 > 主题外观: the theme (默认 plus the bundled sabaki, yasnaya,
    Megapack, Custom and Fast, and the user's own), a preview of the edited background, board and
    stones, line widths, the stone shadow, the three fonts (计算量及其他字体, UI 字体, 胜率目数字体;
    Lizzie默认, 系统默认 or a system font), pure-colour background / board / stones and the image
    files of a named theme (the default theme's images are fixed, as in Java), the background
    blur (as in Java it blurs the panels only when the glass effect is off: `glass-effect-level`
    0 in an imported Java configuration, without Apple style; the default level 1 draws the
    frosted glass, level 2 the liquid glass, both with the theme's imported glass colours and
    blur radii), the graph, comment and best-move colours, the comment font size, the
    last-move marker (圆圈, 三角, 实心, 无), the comment-node colour, the 错误节点 thresholds and
    colours (添加 / 删除 / 重置) and 同时考虑胜率与目数. The edits apply with 保存设置 and are
    dropped by 取消. As in Java, 重置 saves the whole dialog with the default 错误节点 rules and
    reopens it, and 添加 (a new theme) and 删除 take effect at once and reopen the dialog,
    dropping unsaved edits. Bundled themes are read-only: deleting one hides it, and edits are
    kept as the user's copy.
  - 显示 > 外观主题: 棋盘风格 日式 / 中式经典 (corner markers instead of star points, Chinese
    numerals counted from the nearer edge as coordinates), Apple 风格界面 (also turns the Morandi
    colours on; dark translucent menu, tool and bottom bars, darkened wallpaper), 经典配色 (off:
    Morandi colours for the bars, menus and move marks; the bottom bar turns dark), and a custom
    board image (scaled to cover the main board under a light shade; the sub board keeps the
    theme's board) with its 清除 entry. The bar colours change at once; the light toolbar icons of
    Apple style follow from the next start.
  - 设置 > 界面字体大小 (12, 16, 20 or 自定义 12-20) sets, from the next start, the menu font
    size, the toolbar icon size (16, 20 or 24), the font and row height of the main 选点 list,
    and the list rows and fonts of 超级鹰眼 and the 选点列表 window; outside Chinese a size above
    12 first warns that some text may not fit.
  - The fonts follow the UI language: Japanese, Korean, Thai and Traditional Chinese text fall
    back to that language's fonts. 超级鹰眼's charts and lists use the theme's fonts.
  - The variation tree colours nodes by the 错误节点 rules and rings commented nodes in the
    comment-node colour; the win-rate graph uses the theme's line, miss-line, score-line and
    blunder-bar colours and widths. A change from the earlier versions of this client: the
    graph's unanalysed stretches and blunder bars now use the theme colours Java shows (dark blue
    and pink by default) instead of light grey and graded bar colours, and the comment panel
    background is Java's (0,0,0,200) instead of a slightly darker black.
- Fixed Java layout modes: 默认 (`Alt+1`), 经典 (`Alt+2`), 精简 (`Alt+3`), 思考 (`Alt+4`),
  四方图 (`Alt+5`), 双引擎 (`Alt+6`) and 浮动双棋盘 (`Alt+7`). Their panel choices are remembered;
  selecting 默认 restores the standard panels, while enabling the graph, comment or candidate
  list in 精简 leaves that mode without restoring the other hidden panels. 经典 hides the status
  display without discarding its normal-mode preference. Unmaximized windows take Java's mode
  proportions; maximized windows keep their size.
  思考 shows the plain position (stones, move numbers and marks, no candidates or variation) in
  a square on the left; 四方图 shows four sub boards there with the 1st-4th candidates'
  variations, the fourth switching to the heat maps or 仅棋盘棋子 when that sub-board mode is set.
  Each of the four takes its own clicks (next / previous candidate) and wheel (variation
  length). The floating boards paint with the current theme and pass presses, hover and the
  wheel to the workbench; a board popup opened from the floating main board appears in the
  main window.
  浮动主棋盘 (`Alt+Q`) and 浮动小棋盘 (`Alt+A`) are independent board windows with remembered
  bounds. Closing the floating main board in 浮动双棋盘 restores 默认; closing the floating sub
  board brings back the in-frame sub board. 自定义布局 1 / 2 (`Alt+8` / `Alt+9`) save and apply
  Java's panel flags, board position, main-window bounds/maximization and floating-window
  choices/bounds, independently of themes, toolbars and analysis settings. As in Java, applying
  a saved layout chooses 浮动棋盘 when its main board is floating, otherwise 普通布局; it does
  not restore a former 思考 / 四方图 / 双引擎 mode; a board position dragged by hand moves to the
  saved position. Their 设置 dialog and 面板 > 可视化面板设置 preview each change at once; 确定
  saves the panels, board position and (for a slot) the snapshot together, and a save that fails
  keeps the dialog open with its choices. Java configuration import recognizes these modes, the
  classic flag and both saved layouts.
- 独立棋局控制面板 (显示 > 面板, a check item; the top toolbar's control button; also found by
  function search): a separate, ordinary window (title bar, resizable, not always on top) with
  eight large buttons: `|<` / `>|` go to the first / last move, `<<` / `>>` back / forward five
  moves, `<` / `>` one move, 返回 goes back to the main line and 涂鸦 opens 自由涂鸦 on the
  workbench. While a candidate's variation shows, `<<`, `<`, `>` and `>>` step it instead (see
  browsing above). As in Java, `<<`, `<`, `>` and `>>` do nothing during a game against the engine,
  and no button does anything while a modal dialog (综合设置, 初始化设置, a message) is open. The
  menu item and the toolbar button show it or hide it; its close box hides it. It first opens
  510 x 470 centred on the main window, then keeps the place and size it was given; open when the
  application closes, it opens again at the next start. 重置界面位置 forgets its place. Its place
  and open state are this product's own and are not imported from LizzieYzy Next.
- 双引擎模式 (显示 > 布局模式, `Alt+6`; 默认模式 `Alt+1`): a second engine analyses the same
  position next to the foreground engine. The window takes Java's width for the mode (the board
  height doubled, or 1.65 times it for 默认模式) unless it is maximized, and shows two equal
  boards: the foreground engine's results on the left, the secondary engine's on the right, with
  主引擎 / 副引擎 status strips. The secondary engine is chosen in its own menu (`[n]: name`, or
  未加载引擎) with restart and shut-down entries; the same engine cannot be both (第一引擎不能与
  第二引擎相同). Each result belongs to the engine and the node it was requested for, so results
  arriving after fast navigation are not shown on another move. Pausing (Space) and the
  foreground engine's analysis limits stop both engines; the secondary engine's own limits
  stop only it. With fast engine change the previous secondary engine keeps running and is
  chosen again when the new one fails while starting; no secondary engine is chosen during
  setup mode. The game komi, record rules, GTP console commands and 纯网络 apply to both.
  Hovering a point shows each engine's own variation, clicking either board plays there.
  独立选点列表 (`U`) opens a 主引擎 and a 副引擎 window side by side, each with its own engine's
  candidates, actual move and score columns (also from saved results without an engine);
  closing either closes both. Saving writes the secondary analysis as `LZ2` / `LZOP2` (and its
  summary in comments with 记录选点信息到评论中); opening a record that has it in 默认模式 asks
  whether to switch. Leaving the mode stops the secondary analysis but keeps the engine for the
  next time; the mode is remembered across restarts, the secondary engine is chosen again each
  session, as in Java. Changes from Java: the secondary strip shows the secondary engine's own
  开启 / 暂停 and only shows visits and win rate when that engine has a result (Java showed the
  foreground's state and 0 / 50 %), it is no longer cut short when the window is wider than the
  two boards, and the comment summary compares with the secondary engine's own previous result.
- 自动棋谱快析 (综合设置 > 棋谱加载 > 打开棋谱后自动快速分析, on by default): after a record is opened,
  every main-trunk move without analysis is analysed in the background. Which engine does it
  follows Java: a KataGo `analysis` process started for the task, or the foreground engine
  itself when 精析设置 > 复用当前前台引擎 is on or the foreground engine is this client's Windows
  NVIDIA (CUDA/TensorRT) KataGo. A record that declares rules waits until the engine confirms
  them. The foreground engine stays idle while an attempt runs and resumes when the task ends. A
  borrowed engine is taken over with `stop` and its rules are read (without them it is given back
  unsearched); it searches each move to 16 visits (longer games get an overview of every 8th move
  first) with the record's rules or 精析设置's specific rules, and is given back with its own rules
  and the browsing position; only an answered hand-back counts as given back, otherwise it stays
  "引擎状态未恢复" until 重启当前引擎. A failed attempt lets the foreground engine analyse until
  the next attempt, which comes 3.6 s later and twice as late after each further failure, up to
  30 s (the status line says so). Results are stored on the node they were requested for under
  闪电分析, never mark the record as changed and are saved with it like any analysis; results for
  a record that is no longer open, or asked under rules settings changed since, are dropped.
  Pausing analysis (Space) also cancels the record's quick analysis for good (resuming does not
  bring it back; a newly opened record starts afresh); opening another record while the engine is
  borrowed waits until it is given back.
- 精析设置 (分析 menu): the analysis engine command, 单步计算量, current or specific rules, 总是覆盖,
  preload, auto quit and 复用当前前台引擎 with its availability. A command that was never edited
  follows the current engine (`gtp` becomes `analysis` and the config becomes the `analysis.cfg`
  beside it, created from KataGo's example when missing); 选择引擎 makes it the same way from a
  saved engine, and 自动生成 asks for the KataGo program, model and config files. 远程引擎 with
  设置 runs the analysis command on an SSH host instead (see remote engines below); the
  lightweight quick-analysis model is a file of this computer and is not used then. The hint's
  这里 opens KataGo's analysis engine documentation in the system browser.
  - 启动Lizzie时预加载引擎 starts the analysis process with the configured command when the
    application starts, as in Java: at once, or, when the analysis depends on the foreground
    engine (复用当前前台引擎, a cloud foreground engine, or this client's Windows NVIDIA,
    TensorRT or OpenCL KataGo), once that engine is ready. With 复用当前前台引擎 no process starts. Nothing is preloaded while
    analysis is paused or 自动分析, 整盘精析, a game or an engine match runs, or when the engine it
    depends on failed (also failed to launch). A record still being read (at start-up, the record
    given on the command line or the resumed game) is waited for; as in Java, opening an SGF
    record runs its engine sync (at once when there is no engine), whose end ends the preload,
    and a preload waiting for an opened record's rules to reach the engine gives up once they
    have (or were refused). A GIB record has no engine sync. Switching or restarting the engine,
    pausing, starting 闪电分析 or 自动分析 and changing the lightweight model also abandon a waiting
    preload; saving 精析设置 never starts one. Unlike Java, unticking the option while a preload
    waits cancels it. A failed preload is only logged, with no dialog and no retry.
  - As in Java, an idle local analysis process (preloaded, or kept after 闪电分析) ends as soon
    as the foreground engine starts analysing a position or a stone is placed, so it stays only
    while the foreground engine is idle (for example with no engine). One on the SSH host stays.
    The process ends with the application.
- Remote (SSH) engines: 更多引擎's 远程引擎 row (IP, 端口, 用户名, 密码 or 密钥 with 浏览, and the 远程
  column) and 精析设置's 远程引擎 run the command on the host over SSH (a port that is not a number
  means 22). An engine command runs exactly as written; the analysis command gets the same
  `analysisPVLen` and thread overrides as a local one, like Java. Paths, models and files belong
  to the host, so thread benchmarks, GTP configuration detection, 自动生成 and resource
  management are not offered for such engines. The host key is trusted on first use and kept in
  `ssh-known-hosts.json` beside `config.json`; a different key is refused before anything is
  sent, and the failure dialog shows both SHA-256 fingerprints with 信任新密钥并重试, which trusts
  only the key shown there and starts the engine again in the seat it failed in (in 双引擎模式 a
  secondary engine stays the secondary one). Passwords never go into `config.json`: they are kept
  by the system credential store (Windows DPAPI, macOS Keychain, Linux Secret Service), or only
  until quitting when that store is unavailable; the password field shows which and is left empty
  to keep it. A typed password replaces the saved one only when its settings are saved. Remote
  engines refresh at 综合设置 > 高级 > SSH 分析刷新间隔. Failures say what happened: a refused login (Java's
  认证失败), a connection or key-exchange failure or timeout (3 s each), a missing password, the
  remote command ending on the host (its exit code or signal), or a connection lost without the
  command's end, when whether it still runs on the host is unknown (keepalive every 10 s, lost
  after three unanswered). A configured engine whose login or connection failed opens 更多引擎
  afterwards, like Java. Closing or switching the engine sends `quit` and closes its input; a
  command still running 3 s later (also one that has stopped reading its input) is sent `KILL`
  where the server honours signals, and the channel is closed. A remote engine that ends on its
  own or loses its connection is started again over a new connection (自动检测引擎是否存活, above).
- Cloud engines: 设置 > 远程算力, the bottom toolbar's 远程算力 button and 引擎一键设置's
  远程算力 entry open the same non-modal center. 智子云算力 supports password/code sign-in,
  password reset, account balances, usage and credit details, available models, membership
  and balance orders. 自建算力 takes a `ws://` or `wss://` KataGo Analysis WebSocket link,
  including one imported from a QR image. It is not a raw GTP-over-WebSocket endpoint.
  - Sign-in and account refresh do not start an engine. 启用智子云算力 or 使用此远程链接
    explicitly selects and starts the configured service; 创建订单 separately authorizes an
    order. Failed or uncertain order creation is not automatically retried. Closing a payment
    window stops local polling, not an order already created by the service. Check the account
    or order before paying again. Service fees and remote resource lifetime remain with the
    provider; closing a socket is not proof that a billed resource has been released.
  - Remember-token and remember-password choices are independent. Secrets use the native
    credential store, with a visible session-only fallback when it is unavailable; neither is
    written to `config.json`. Logout clears the account credentials. A custom link can itself
    contain a credential: its saved URL is part of this client's private configuration, so do
    not share that file. Diagnostics omit passwords, tokens and credential-bearing URLs.
    When the saved startup choice is Zhizi but no token is available, startup uses the first
    usable local engine for that session, if one exists. The saved provider, default engine and
    last-engine choice stay unchanged.
  - Switching replaces the foreground engine and synchronizes the current board, komi and
    rules. Automatic quick analysis borrows a cloud foreground engine rather than launching a
    separate local process. A switch succeeds once the new engine has confirmed the board
    sync; only then does successful cloud/local switching run the normal analysis
    preload entry. 切换到本地引擎 selects the first local profile whose executable and referenced
    model/config/JAR files exist. 引擎 > 关闭当前引擎 disconnects it; closing the center alone
    leaves an established engine running. Closing the engine while a switch is still starting
    it (for example 关闭所有引擎), or switching to another engine, ends that switch: the center
    becomes usable again and reports that the switch did not complete.
  - Zhizi uses HTTPS account operations and its Socket.IO v4 transport. A startup has at most
    three connection attempts with a fresh socket token for each; temporary token-service
    failures share that budget, while fatal authentication or service rejection does not retry.
    Account HTTP and both cloud WebSocket transports use the selected direct, system-proxy or
    manual-proxy mode. Invalid or unavailable manual proxies never fall back to a direct connection.
    An established Zhizi analysis that has no accepted visit
    progress for 20 seconds triggers the foreground recovery path, independently of the ordinary
    alive-check preference. Late data from a retired connection cannot update the board. Custom
    WebSocket failures use the normal engine failure/restart policy. On a custom link, any new
    command ends a running analysis query, as on a local KataGo, and the service is told to
    terminate it, so browsing to another position starts that position's search; an engine move
    asked with analysis keeps that move's evaluation. An explicit `stop` is confirmed only after
    the service sends that query's final result and its terminate acknowledgement, even when later
    commands were already answered; when that happens while a newer analysis is reporting, the
    confirmation follows the end of that analysis, so its reports stay with it. A refused
    terminate fails the custom engine, as in Java.
  - Local protocol fixtures and the Linux desktop exercise these paths. Authorized live Zhizi
    and independently hosted custom-service acceptance, including real billing, have not yet
    been completed; fixture success is not a claim of current provider availability.
- 闪电分析 (分析 menu and the toolbar's lightning button): 整盘速览 (`Ctrl+B`) asks every main-trunk
  move, 局部精析 asks from its start move up to, not including, its end move (an empty field means
  from the first / to the last move; the range is remembered), and 全部分支精析 asks every move of
  every branch. Moves that already have more than 精析设置's 单步计算量 are skipped unless 总是覆盖 is
  on. Like Java the run uses 精析设置's analysis engine (a running quick analysis ends and hands
  its process over) or, with 复用当前前台引擎, borrows the foreground engine the same way quick
  analysis does; without an analysis command it offers 精析设置 first and then starts. A
  progress window (进度 n/m) blocks the workbench: 隐藏界面 hides it while the run continues,
  取消分析 ends the run and keeps the answers so far (an analysis process ends and the foreground
  engine it stopped stays paused until resumed; a borrowed engine is given back), and GTP控制台
  stays usable. Completion shows 分析完毕! for a second, steps from the root to the first
  move and lets the foreground engine analyse again unless it was paused. A rejected request ends
  the run with Java's failure message; a failed analysis process shows the engine failure and then
  精析设置 with 确定并重新计算; a borrowed engine that cannot be given back stays "引擎状态未恢复".
  Changing 精析设置's rules during a run ends it as a failed request, as in Java: answers asked
  under the old rules are not stored and browsing does not step to move 1.
  Results are stored like quick analysis results; opening another record ends the run silently.
  Each run searches the positions as they were when it was started: a position edited, deleted
  or added while it runs gets no result from it, answers land only on the node they were asked
  for, and browsing stays where the user went (only completion at the root steps to move 1, as
  in Java). A cancelled run's late answers never count for the next run.
- 强制计算区域 (toolbar region buttons, Alt+drag, the board's right-click menu): 设置强制计算区域
  / 设置强制不计算区域 lights its button and the next drag on the board selects the intersections
  inside the rectangle (Alt+drag always selects a calculation region; Alt+click clears an existing
  region, like 清除强制区域). The foreground KataGo request then carries `allow`/`avoid` for both
  colours; the depth comes from each button's popup (限制整个搜索过程 999, 仅限制第一手 1, or
  自定义限制手数, saved as `select-*-moves`). The board shades the region. A toolbar or Alt
  selection stays for later positions until cleared; a right-click 只分析此点 / 增加分析此点 /
  去除分析此点 (depth 1) or 不分析此点 lasts for that request only unless 持续分析/不分析 is
  checked; 清除分析/不分析 clears it. Regions are session state: they never change the record and
  belong to the open record only (批量分析's searches of its own records never carry them).
  Stored results of the position give way to the next restricted live result.
- 选点重点研究 (right-click 评估此点 / 取消评估 / 清除所有评估选点 on an empty point): on a local
  KataGo whose `kata-analyze` accepts `focus`, each tracked point gets extra search until its
  candidate reaches 选点重点研究累计计算量目标 (综合设置 > 引擎与分析, default 500) or makes no
  progress for 8 seconds, counted from the current search (a stored result with more visits does
  not count, and while points are tracked the position shows the current search's result).
  Adding or removing points re-sends only the analysis request, so KataGo keeps its search tree
  and the ordinary analysis time limit keeps counting; the engine is not restarted. The board
  draws a dashed outline in the move-rank colour of the point's candidate against the best one
  (综合设置 > 主题 > 关注点质量外框, with its opacity) and a blue progress ring while it is searched.
  Tracked points belong to one position, engine and setting: browsing, editing, komi/rules
  changes or another engine end them, and pausing ends the extra search. Support is decided by
  each engine instance's own probe (`kata-analyze … focus pass 0`, then `stop`) when it starts,
  restarts or becomes the foreground engine, paused or not; the probe's output is never shown as
  analysis and never resumes a paused engine. An engine that rejects `focus` (for example the
  official v1.18.1 release build, or a 自建算力 link, whose adapter rejects the probe without a
  remote query) keeps ordinary analysis, and the menu item is disabled as
  选点评估需要升级引擎; a failed probe says so and is repeated by restarting the engine. This
  client does not ship a KataGo build with `focus`.
- 自动分析 (`A`, 分析 > 自动分析, or the bottom bar 自动分析 popup) opens Java's single-record
  settings: start/end move, per-position time, total visits or first-candidate visits (any limit
  advances), Black/White, all branches, stronger analysis on large fluctuations, pause-exit and
  automatic SGF saving. An empty start means the current position; a positive start begins at
  the position before that move. Defaults are start 1, 2 seconds, both sides, main line,
  pause-exit and auto-save on. It uses the normal foreground GTP engine, not another analysis
  process, and hands a running quick analysis back first. The board follows its progress;
  reverse-child depth-first traversal visits side branches before the main continuation.
  Stop (分析 > 终止自动分析 or the settings dialog) keeps the current position and results.
  Natural completion optionally strengthens the before/after positions of sufficiently large
  win-rate or score changes; forced stop asks before that phase. Space exits without saving when
  手动暂停分析则退出自动分析 is on (the exit notice can be disabled); otherwise it pauses the
  run until analysis resumes. Record or engine replacement ends the run without transferring it;
  positions deleted after the run planned them are skipped.
  Automatic saves after at least one completed position use the existing serial save lane:
  `<name>_已分析_<yyyyMMddHHmmss>.sgf` beside the source, or `AnalyzedGames/<timestamp>.sgf` in
  this client's application data directory for a new record. They are copies, not a rename of
  the source. The root gets Java's AI-match/top-ten-difference summaries for sides with at least
  ten evaluable moves; like Java, the open record shows them too without becoming unsaved.
- 批量分析 (`Ctrl+O` on the board, 分析 menu or the 自动分析 popup; also several record files
  dropped on the window) and 批量精析 choose several SGF/GIB files; as in Java, the first record
  is read before Java's settings dialog opens (an unreadable first record ends 批量分析 without
  the dialog; stopping automatic analysis during that read cancels the batch). As in Java,
  choosing or dropping records for either batch, and 进度表's 开始/结束, check 自动保存棋谱 for
  single-record analysis before the dialog shows, even if it is then cancelled; 开始/结束 starts
  批量分析 of the remaining rows the same way, reading their first record again. 批量分析 uses
  the 自动分析 fields (its start move defaults to the first move and
  its records are always saved); 批量精析 asks for the move range, 每手总计算量
  (`batch-analysis-playouts`, default 100; a search uses at least 2) and 分析所有分支, and its 设置
  button edits that value in 精析设置. Each record is read
  and analysed in turn, then saved beside its source as `<name>_已分析_<yyyyMMddHHmmss>.sgf`
  (always SGF; the source is not changed, and the copy keeps the source's secondary-engine
  analysis, `LZ2` / `LZOP2`, with all of its candidates whatever 最大候选点数 is, as Java reads
  those tags; variations follow 变化长度 as in Java). Unlike Java, the records are analysed as background
  documents with their own komi and declared rules: the open record stays on the board and can
  be browsed, edited and saved meanwhile, and nothing the batch does marks it saved or moves it.
  批量分析 runs automatic analysis with the foreground engine (which follows the batch record
  while it runs); 终止自动分析 or 进度表's 开始/结束 asks about strengthening as single-record
  analysis does, saves the current record and ends the batch (while the next record is still
  loading, it ends the batch there). 批量精析 uses the 精析设置 analysis process (or the borrowed
  foreground engine, as chosen when the batch started) for the whole batch and shows its
  progress window; 取消分析 ends the engine without saving the current record, which stays first
  in 进度表 with the results it has, so 开始/结束(闪电模式) continues from it and asks only for
  the positions still below the requested visits (all of them with 总是覆盖已有分析结果). A
  borrowed engine searches a record that declares no rules with the rules it had when it was
  taken over, not with those of the record before.
  批量分析进度表 lists the current record and the ones still to
  analyse (finished ones leave the list), with the current record's progress, 增加棋谱, 清空
  (which stops a running batch first), and per-row 优先 / 上移 / 下移 / 删除; it opens by itself
  when 批量分析 of several records starts, and closing it leaves the batch running. A record
  that cannot be read ends 批量分析 naming that file, as in Java; 批量精析 reports it and goes on
  with the next record (Java analyses the previous record again and saves it under the
  unreadable file's name). Unreadable records and any save that fails are listed when the batch
  ends, and records already saved stay; a new batch starts only after the previous one's copies
  are written. The queue is not kept across restarts (Java keeps it only in memory). While a
  batch runs, including while its next record loads, single-record automatic, flash and
  whole-game analyses are refused with Java's conflict message. While a game against the engine
  runs or gives its engine back (or another mode holds the engine), choosing, dropping or
  starting either batch is refused with that mode's message before any record is read, as Java
  refuses loading a record in a game.
- 整盘精析 (分析 menu, `Ctrl+Shift+B`, or 自动分析 popup): a modeless dialog selects 500, 1,000,
  3,000, 10,000 or a custom 500–1,000,000 visits per position. It first fills the missing
  32-visit overview of the starting position and every main-trunk move, then searches missing
  deep results; real passes count, dummy passes and side branches do not. Deeper cached results
  are reused even with 总是覆盖 enabled. The
  progress bar gives the overview 20% and deep analysis 80%; a stored engine benchmark supplies
  the pre-start estimate, otherwise the dialog shows the ordinary visits hint. 暂停分析 closes
  the process or returns the borrowed foreground engine; 继续分析 re-plans only missing results.
  停止分析 keeps all completed results. Closing the dialog hides an active session; reopening
  shows it again. Completion closes it and shows a five-second result notice; with a borrowed
  engine this happens once the engine is confirmed back (until then progress stays at 99%), and a
  failed return reports the engine error instead. Browsing remains
  available throughout. The analysis process uses KataGo's whole-game throughput thread profile;
  an explicitly accepted matching whole-game measured profile takes precedence after off-thread
  artifact/hardware verification (stale or ambiguous profiles use the normal thread profile).
  复用当前前台引擎 uses the same sequential foreground lease as quick analysis; a borrowed search
  is given up only after 8 s without any report from the engine (one that keeps reporting may
  take longer, as in Java); switching the
  foreground engine during such a run, or while it is still returning the engine after its last
  answer, stops it with its results kept; the new engine analyses at once. The foreground
  engine stopped by a dedicated run resumes on completion, cancellation or failure unless the
  user paused it. Main-line edits, rules/komi changes or replacing the record stop the task
  safely; game-info and komi editors refuse changes during the session. Analysed nodes show
  suggestions, variation previews, win rate/playouts and move evaluation without changing saved
  display preferences; this override ends on another document. Results are labelled 整盘精析,
  never make the record unsaved, and remain available for an explicit SGF save. With 精析设置's
  远程引擎 the run uses the remote analysis engine (measured tuning describes this computer and
  is not applied). The chosen depth is saved on start in `wholeGame.deepVisits`;
  invalid stored values fall back to 1,000, matching Java.
- 人机对弈 (棋局 menu, `N` / `Alt+N`, the toolbar's 新对局 button): Java's 新对局(Genmove模式) and
  分析模式 dialogs choose the engine, colour, 接着当前局面下, handicap (KataGo may choose the points
  with 自由让子), komi, 限制我的用时 (保留时间, 读秒 and 次数, with Java's countdown voices), the
  engine's time (每手用时, a raw GTP time command, KataGo's 读秒/加秒/包干 or the engine's own
  time; the 自建算力 engine takes only 每手用时 or its own time, and a Genmove game or 续弈 with
  another clock is refused before anything is saved, with Java's message), KataGo visit limits,
  后台思考, 对弈模式 (analysis hidden during the game), the candidates
  shown for each side and 自动保存棋谱; 分析模式 adds 纯网络, Java's resignation thresholds, 不使用
  分析广度拓展 and the 落子条件 (time, total or first-candidate visits) shared with the bottom bar's
  自动落子. A game has started only once the engine answered its setup (moves are refused with
  Java's "wait for the engine" message until then). The engine's move becomes the record's next
  move only if it answers the game, record and position it was asked for; a late answer after
  悔棋, a new board, the end of the game or an engine exit changes nothing. 停一手 (`P`), 暂停 /
  继续, 悔棋 (back to the human's previous turn), 认输 (no confirmation), 结束对局 (`Space`) and
  the engine's own resignation or the human's timeout end the game as Java does: the result
  (B+R, W+R, B+T, W+T) is recorded, the played line becomes the main line, the game is saved to
  `MyGames/<time>(<black>_Vs_<white>).sgf` in this client's application data directory, and
  analysis stays paused; a finished game is counted with 棋局 > 终局点目. While a game runs or
  gives its engine back, opening records, browsing, engine switching (also a resource plan's
  立即切换; saving the plan without switching stays available), komi/rules changes and the
  other analysis modes are refused; the engine is given back only after it answered the game's
  setup and confirmed the restore of the parameters the game changed (`maxTime`, visit limits,
  `analysisWideRootNoise`). A refused or unconfirmed restore leaves the engine marked unrestored;
  when the engine refuses the position (`play`) behind which its move was asked, the game ends
  as failed and that move's answer is ignored (Java fails an engine game the same way);
  an engine that never answers the move it is thinking about can be restarted or closed to end
  the wait. Opening a 新对局 dialog ends a running game and pauses analysis (cancelling resumes
  it); an 分析模式 game turns 自动落子 on for the engine's side and off again at its end, and
  终局点目 stops the game's analysis. Enter / `Alt+Enter` and the 续弈 menu items continue from
  the current position; 修改AI用时, AI随机开局 and `Alt+,` (one engine move) are available.
  Differences from Java: results are always SGF (`B+R`, not the localized Japanese, Korean or
  Thai text Java writes there), the dialogs' switches are saved only with 确定 (Java saved some
  even when cancelled), a new board during a game ends the game first, AI随机开局 weights
  close candidates by their share of the most searched candidate's visits (Java compares with
  that count only when its unrelated `check-random-visits` option is on), and 接着当前局面下 keeps
  the setup stones (`AB`/`AW`/`AE`) and side to play (`PL`) set between moves on the way to the
  current position (Java replayed only the moves, so the game could start from another position).
- 引擎对战 (棋局 > 新对局 > 引擎对局, `Alt+E`, the detailed toolbar's 引擎对战 panel): two different
  engines of the engine list play each other in Java's Genmove模式 (the engines' own `genmove`
  with per-side 每手用时 (sent to the 自建算力 engine as KataGo's per-move `maxTime`), the
  engine's own time or a raw GTP time command) or 分析模式 (the mover's
  analysis plays when its time, total or first-candidate visits are reached, with per-side
  resignation thresholds and 随机落子). The dialog, 引擎对局设置 (from the dialog or the panel's 设置),
  the panel, 人工干预 (立即落子, 允许人工落子, 黑认输, 白认输), 终止 / 暂停继续引擎对局 and
  修改多盘引擎对局数 follow Java, including 本局规则 (verified on KataGo with `kata-get-rules`; an
  engine that cannot confirm them asks for consent), komi, handicap (on a 19×19 board only; when
  the black engine's board is another size the game starts without handicap), 多盘对局 with
  交换黑白, 当前局面续弈, 加载开局SGF (in order or at random), 最大手数 and 后台计算. Both engines run
  as their own processes (other running engines are closed); the engine to move is the
  foreground engine, so its analysis is what the board, the lists and the graph show. A match
  is accepted before it starts: it starts only once both engines are ready and answered their
  setup. A move counts only if it answers the game, record and position it was asked for; a
  late answer after 结束, a new game or an engine exit changes nothing. 暂停 lets a Genmove move
  already being chosen land (as Java) and asks for no further move until 继续. A game ends by
  resignation, two passes, a full board or 最大手数 (the board area × 2 when off or 0);
  two-pass and move-limit games are saved but not scored.
  Games are saved as Java names them under `EngineGames/` (a batch under
  `EngineGames/<batch name>/`, default `<first>_VS_<second>_<time>`) in this client's application
  data directory, with Java's move comments, together with the batch's `结果<time>.txt` summary
  (score, wins as black/white, time, visits, Elo and confidence intervals) and, with 自动保存胜率图,
  the winrate graph. The saved record's 本局规则 block and the summary name each engine with its
  command as Java does, with credential values in it (`--token …`, `password=…`) masked by the
  credential sanitizer. The next game of a batch starts only after both engines confirmed they are
  given back (parameters and rules the match changed are restored; a refusal or missing
  confirmation marks the engine unrestored and stops the batch); an engine exit retires the
  game and ends the batch (the exit is logged, without the 引擎异常关闭 window), after which that
  engine starts again (back in the foreground seat when it held it and no engine has taken the
  seat since, synced to the position then shown); a failure to start it is only logged and it is
  not tried again. While a match runs, opening records, browsing, engine switching, editing and
  the other analysis modes are refused.
  Differences from Java: results are always SGF `B+R` / `W+R`; the batch summary lists each
  engine's own settings after colours are exchanged (Java mixed the black and white sides there);
  a custom batch folder name made only of dots or separators is refused, so files stay under
  `EngineGames`; 当前局面续弈 and 加载开局SGF keep setup stones and side to play set between
  moves (Java replayed only the moves).
- AI 陪练 (HumanSL; the menu bar's AI 陪练 button, 棋局 > 新建对局 > AI 陪练, function search): a
  game against KataGo's human-imitation (HumanSL) network on the main board. The setup dialog
  follows Java: 训练方式 (实战后复盘 hides candidates, the winrate graph, Kata形势, policy and
  heat map and pauses the foreground analysis during the game; 边下边分析 keeps the foreground
  analysing with both sides' candidates shown; neither changes the saved display settings),
  AI 对手 (by rank, 20 kyu to 9 dan, or 职业棋风: 现代职业棋风 `proyear_2023` / 网络 9 段), colour
  (random, black or white), the AI's time per move (10 s, 30 s, 1 min, unlimited), and under 更多
  设置 handicap and komi; 从当前局面开始 (only when the current position has moves) continues the
  current record in a new variation, otherwise a new record is started. The last started
  settings are remembered. The AI's moves come from a separate `katago analysis` process with
  `-human-model`, started from the 精析设置 analysis command (a Windows TensorRT engine uses the
  installed CUDA package instead); the game starts only after that process answered a real
  HumanSL request, with Java's loading stages shown meanwhile. Moves follow Java's human-like
  selection (HumanSL policy candidates checked by a short search; no AI pass before move 200,
  800–4000 ms thinking pace); an AI answer for a position that is no longer current ends the
  session without touching the record. A companion process that exits is started again for the
  next move; two failed attempts show 重试 AI. The bar under the board shows the opponent, both
  clocks and the turn, with 停一手 (`P`) and 结束并复盘. Ending, cancelling or a failed start stops
  the companion and gives the foreground engine back as it was (after 实战后复盘 analysis resumes
  on the finished game); a failed restore offers 重试清理. Opening a record, a new board or another
  game during a session first ends the session, then proceeds. The training report and live
  correction strings present in Java are not reachable there and are not shown here.
- Switching between 人机对弈, 引擎对战 and AI 陪练: two of them never run together. During a game
  a match or AI 陪练 is refused and a new game ends the running game first; during a match a
  game or AI 陪练 is refused; a new game or match asked during AI 陪练 ends the session first
  (see above). A mode also waits until the engines of an ended one have confirmed they are
  given back. A game or a match does not start while 自动快析, 闪电分析, 整盘精析, 自动分析 or a
  batch uses the foreground engine (the "engine already in use" message). AI 陪练 starts beside
  them as in Java: as a new game, its new record ends quick, flash, whole-game (failed as the
  game changed) and automatic analysis once its opponent is ready; from the current position, the
  start ends them first (stopped like their own stop, results kept). During a batch it asks to
  stop the batch first (停止并开始 stops it as 停止批量分析 does, keeping finished results), because
  in Java a batch outlives the coach's new board. A move, an AI 陪练 answer or an AI 解说 reply that
  arrives after its mode or its record has ended is dropped and never reaches the record that
  follows. Back in review, analysis stays paused after a game and resumes after 实战后复盘.
- HumanSL model (引擎一键设置 > 权重管理, or 下载并开始 in the AI 陪练 dialog): the official
  `b18c384nbt-humanv0.bin.gz` (99,066,230 bytes) is downloaded from katagotraining.org on demand
  and accepted only with the catalog's size and SHA-256; a file with the official name but another
  digest is reported as missing and never used. 导入 copies another `.bin.gz` / `.txt.gz` HumanSL
  model (larger than 1 MB) into `human-sl-models/` of the resource directory. The model is not an
  analysis weight and is not offered as one.
- 棋力评估 (分析 > 棋力评估, the top toolbar's 棋力评估 button, function search): estimates each
  side's playing strength from the analysis already stored on the main line (it starts no
  computation; analyse the game first, for example with 闪电分析整局). The dashboard follows
  Java: 测评 (rank, confidence and per-side metrics), 吻合度 (overall match and the per-move hit
  map), 发挥水准 (move quality distribution), and 详细数据. The default model is Java's bundled
  XGBoost 20TUN with its residual calibrator; on the same analysed game it gives exactly the
  fixed Java build's numbers. 导入 adds another XGBoost rank-value booster (`.json`, with a
  sibling `metadata.json` / calibrator if present, or a `.zip` bundle) under `strength-models/`
  in the data directory and selects it; the chosen model lasts for the session, as in Java.
  HumanSL 40 is Java's experimental model: 补算 HumanSL runs a separate policy-only
  `katago analysis` process on the official HumanSL network (download offered if missing) for
  the first 210 moves of a 19×19 game without setup stones, with a progress dialog that 取消
  stops. Policies are cached per query under `strength-humansl40-policy-cache/`, so the same
  game is not queried again. 后台 HumanSL (off by default, saved) prepares the same data while
  the foreground engine analyses, and stops when analysis is paused or a game, engine match or
  AI 陪练 needs the engine. Its HumanSL policies come from network sampling, so its ranks can
  differ slightly between runs. As in Java, the other models read only the candidates within
  最大候选点数 (plus the one the board keeps after it just dropped out, and the points
  选点重点研究 tracks), and a move that is no candidate is scored from the two positions' evaluations; when
  only one of them has a score (for example a Leela Zero position next to a KataGo one), this
  client uses the win-rate difference instead of Java's score of 0 for the scoreless position.
- AI 解说 (分析 > AI 解说, the AI 解说 button after AI 陪练 in the menu strip, shown as 解说 when the
  strip is narrower than 900 px, function search): explains the analysis already stored on the
  record in plain language through a language-model service you configure. It starts no engine
  computation, so analyse the positions first. The window follows Java: 下一手 compares the
  recorded next move with the first three KataGo candidates (plus the played move when it is a
  lower candidate), 区间 and 整局 pick at most five key positions of the main line, at least three
  moves apart, by score loss (then win-rate loss, then visits), and 继续追问 asks about the current
  explanation with the same evidence. 分析依据 shows exactly what was sent: candidates and losses,
  stone groups and liberties, the replayed continuation and candidate lines, and up to two grounded
  matches from the bundled common-pattern teaching catalogue. A reply that names a coordinate
  outside that evidence is sent back once for correction; a second unsupported reply is reported
  as unverified and not saved. Verifier notes on an accepted reply are shown, never saved. With
  将解说加入棋谱评论 checked (the default), the answer goes into the node's comment as a separate
  `[LizzieYzy AI Commentary BEGIN]` … `[LizzieYzy AI Commentary END]` block that replaces an earlier
  block and leaves your own comment untouched; save the record to keep it, and the comment panel
  shows it as its own AI 解说 section. The block goes to the node the request was made for (下一手
  and 区间: the current node; 整局: the root) even if you navigate meanwhile; if that node is deleted
  or its position is edited after its evidence was taken (also when 继续追问 reuses that evidence),
  the answer is shown but not stored (Java stored it into the stale node). 停止, closing the window
  or opening another record cancels the request, closes the connection, ends the reply reader and
  discards a late reply.
  设置 > 连接服务 offers Java's two connections. **API Key** works with any OpenAI-compatible
  service: service address (`/v1` is added to an address without a path), key and model; 获取模型
  lists `/models`; requests stream from `/chat/completions` and fall back to `/responses` only when
  the service answers 404 or 405; HTTPS is required except for a service on this computer. The
  key filled in from this session belongs to the saved address: changing the address empties the
  field, so 获取模型 never sends it to another service, and a model list still loading when the
  address or key changes is dropped (Java sent the filled key to the new address and offered the
  old address's models for it).
  **ChatGPT 登录** is OpenAI's Sign in with ChatGPT in the browser and uses your ChatGPT plan
  (管理用量 opens its usage page); several accounts can be added and switched. 记住密钥 keeps the key
  in the system credential store (Windows DPAPI, macOS Keychain, Linux Secret Service through
  `secret-tool`); without one the key lasts for this session only. Keys and sign-in tokens are
  never written to the settings files or logs. 讲解偏好 (level, style, terminology, pace, variations)
  only shape the prompt. 导入 Java 配置 migrates Java's AI commentary settings; a remembered Java API
  key is copied only when you tick it, and ChatGPT sign-ins are not migrated (sign in again).
  Accounts and costs belong to your provider; nothing is sent until you start a commentary.
- 死活题 (分析 > 死活题, Shift+E, the top toolbar's 死活 button, function search): frames the
  current position for engine reading like Java's tsumego frame. The attacker (自动检测 picks the
  colour with more stones, Black on a tie, as Java's detection effectively does) walls in the
  stones' bounding box at the chosen 墙面间距 and fills the side of the board nearest the problem;
  the defender fills the rest. 添加劫材 adds Java's small corner shapes for either side and names
  a side whose shape has no room. 确定 replaces the open record with a new game holding only that
  position: moves, variations, names and the root comment go, the side to play is kept or set by
  先手方, and the komi becomes the frame's (7.5 for the attacker, or twice the attacker's area beyond
  half the board, negated for White; kept within ±150) and is sent to the engine like a typed
  komi. A position with stones out to all four edges is left as it is. The dialog's choices are
  remembered. Framing is refused during a game, an engine match, AI 陪练 or 整盘精析. The frame
  matches the fixed Java build's output exactly on randomized positions (7–19 lines, every option).
  Java's habit of going back before an earlier frame never applied after a real frame and is not
  kept.
- 抓取死活题 (分析 > 抓取死活题, the 死活题 dialog's 屏幕抓取, Shift+T): reads a problem shown by any
  other program. 开始抓取 minimizes the main window, takes one picture of the primary screen and
  shows it full screen (dimmed): click the corner intersection (0,0), then the opposite corner of
  the first square, then move until the blue grid covers the problem and click. Stones are read
  with Java's helper rules (near-gray pixels of a square-sized box: enough dark pixels make black;
  enough light ones make white when the box shows a stone's edge or shading, not a flat white
  background) and the 参数设置 thresholds. The problem lands in the board corner it came from on a
  new board (square of the captured width when the board is smaller), Black to play, and 死活题
  opens. Escape or a zero first square cancels and the instructions return. Java ran a separate
  Java helper and re-captured the live screen while the pointer moved; here the picture is taken
  once by the client itself: GDI on Windows, Core Graphics on macOS (needs the Screen Recording
  permission; a one-time hint says so), `GetImage` on X11 and the desktop portal's screenshot on
  Wayland (the desktop may ask for permission). Only the primary screen is used, as in Java.
- Web 旁观 (同步 > Web 旁观, the bottom bar's 同步 popup, function search): 启动 Web 旁观 serves
  Java's viewer page to browsers on the network, at the address shown after the window title
  (`| Web: http://<this machine's address>:9998`) and copied by 复制访问地址. The address is that of
  the interface the system routes outward through; on a network without a default route it is
  the first IPv4 address of an interface that is up and not loopback, and only without any such
  interface `127.0.0.1`. The page listens on
  every network interface; the page uses port 9998 and the board stream 9999, each moving up to
  nine ports further when taken. The ports and the limit of 20 viewers come from settings
  (imported from Java's `web-board` section) and are not edited in the client. Viewers see the
  shown position, its candidates, Kata形势, win-rate/score graphs and 问题手 list. Only the page's
  own three files are served, never records, settings or anything else, and nothing is sent
  unless the share runs. The page has no password, so anyone who can reach the address can watch.
  A viewer may 进入试下: one viewer at a time plays variations from the shown position; they are
  added to the open record as side branches, never the main line (a position without continuation
  first gets an end placeholder, removed again when nothing was played there). The desktop and
  every viewer follow them and the foreground engine analyses them. While it runs, desktop moves
  and 贴目 changes are refused with Java's 3-second hint, desktop browsing waits, and the GTP
  console, switching, restarting or closing the foreground engine (also through 远程算力 or
  applying a settings import), saving the engine list and modes that take the engine (人机对弈,
  AI 陪练, 引擎对战, flash/automatic/whole-game analysis, batch, tracking, benchmark, 跑谱贡献,
  GMA) are refused; an engine that ends meanwhile is not restarted automatically. It ends at the
  viewer's 退出试下, after five idle minutes, by
  强制结束试下, when the share stops or when another record opens; browsing then returns to where
  it started. 自动落子 does not play while a 试下 runs, as desktop moves are refused; it plays
  again with its settings once the 试下 ends (Java still played, on the position the 试下 started
  from).
  Since Java placed only the first placeholder, a 试下 continued past the record's last move could
  extend the main line there; here it stays a branch.
  A 试下 needs a free foreground engine that has finished starting, so none starts while the
  desktop switches or restarts the engine, nor while 野狐 AI 直连 owns the record, from 开始 on,
  before its first position arrives; the viewer gets the engine-busy answer (Java admitted the
  试下 while Fox only listened). 停止 Web 旁观 or quitting closes both ports and disconnects
  every viewer; messages of a stopped share never reach a later one. A firewall between viewer and
  this machine must allow both ports.
- 快速曲线轻量模型 (引擎一键设置 > 权重管理): the official lightweight b10 model is downloaded on demand
  (size and SHA-256 checked), which turns 自动使用 on. A model file is used only while its SHA-256
  matches the catalog (checked again when the file changes); with 自动使用 and a KataGo of 1.17 or
  later in the foreground, quick analysis runs it in an analysis process of that KataGo.
- 问题手 (the 评论/问题手 panel): moves from move 2 on the main trunk whose player lost at least 10
  win-rate points against the analysis of the position before, best moves excepted, for black or
  white, sorted by loss or move number, with Java's empty states and the 评估中 / 已评估 / 评估完成
  badge. Clicking a row goes to the position before the move and marks it. The page, side,
  order and the 显示评论/问题手面板顶部控制条 choice are remembered.
- 超级鹰眼 (`Y`, 视图 > 面板 and 分析 menus, the top toolbar's eye button, the bottom-bar button when
  enabled): a separate window on the analysis already stored with the record (nothing is
  analysed for it). Its 统计信息 page shows each side's 吻合度, 吻合率, 第一选点率, average score and
  win-rate loss and the loss distribution; 简易列表 lists Black's and White's moves; 详细列表
  lists the checked sides' moves (12 columns with KataGo, 9 without), sortable by every column
  (the change columns cycle largest change, largest gain, largest loss). A move counts only when
  the position before it is analysed, its losses only when both positions are, and a
  first-candidate move loses nothing. 列表筛选 sets the win-rate / score / visits thresholds of
  the lists and 置顶当前手; 统计阈值 sets the loss bands; 分支 chooses the main trunk or the current
  line; the 统计 range is 当前, 全局, 布局, 中盘, 收官 or a custom move range. Below, nine charts
  (胜率吻合, 吻合度走势, loss distributions and statistics, 分段吻合度, 翻车现场) can be clicked, dragged
  or scrolled to browse. Clicking a row goes to the position before the move and marks it; under
  主干 only main-trunk moves can be chosen; during 自动分析 row clicks are ignored as in Java, chart
  clicks are not. A click on one record that arrives after another record opened is ignored. A value the stored
  analysis does not have (for example the score of a position analysed by Leela Zero) is an empty cell rather
  than Java's 0. Right-click a list for 导出为xls (a UTF-16
  tab-separated file named `.xls`, as Java writes it). `Q` keeps the window on top. 双引擎模式
  opens a second window on the secondary engine's analysis. The open state, page, chart, filters,
  range, the primary window's bounds, sort and column widths are remembered; the window reopens
  at start when it was open. Java's `badmoves-always-ontop`, `show-badmoves-frame`,
  `movelist-*`, `move-list-*`, `moveList-selected-branch`, `show-*-matchai`, `opening-end-move`,
  `middle-end-move`, `game-statistics-custom-*`, `*-diff-range*`, `loss-panel-select-winrate` and
  `match-ai-firstmove/lastmove` are imported (an unmatched range selects 其他 with the imported
  custom moves, 10–200 by default, as Java does); window coordinates are not. Difference from Java:
  总在最前 is remembered (Java saved it under `badmoves-always-on-top` but read
  `badmoves-always-ontop`, so it never survived a restart; the former key is reported as unused).
  Not yet available: the 显示独立超级鹰眼 settings checkbox (with the settings dialog) and the
  engine-game hint.
- Rules: an SGF record's `RU` is applied to KataGo (`kata-set-rules`) before analysis. When the
  engine cannot confirm it, a dialog asks whether to continue with the engine's current rules
  (no pauses analysis). `Shift+D` opens the KataGo rules editor.
- GTP console (`E`) shows the engine traffic and sends commands. Like Java, `komi <v>` sets the
  game komi and `undo` steps back in the record; other commands that would change the engine's
  board (`play`, `clear_board`, `place_free_handicap`, …) are refused because the record owns the
  position. 完整日志 with the 引擎/GTP scope writes the GTP traffic to `logs/engine-trace.log`. With 控制台隐藏时仍记录
  GTP (综合设置 > 高级 > 日志与分析刷新, on by default) turned off, a hidden console records only engines
  that are still starting; once an engine is ready its traffic appears only while the console is
  shown. A failing
  engine shows Java's failure dialog with its output, copy and diagnostics export. Engines are
  ended (`quit`, then killed after 3 s) when the application exits, including application Quit.
- Logs, the full-log files, failure details and exported diagnostics pass through a port of
  Java's credential sanitizer (`--password …`, `token=…`, JSON credential keys, authorization
  headers, URL user info, encoded forms); the live console shows raw traffic like Java.
- Importing LizzieYzy Next settings: offered at the first start (no settings of this client
  yet) and in Help > 从 LizzieYzy Next 导入设置…. Choose the Java settings folder (the one with
  `config.txt`; Java's per-user folders such as `~/.lizzieyzy-next` and, on Windows, the shared
  `LizzieYzyNext` folders are suggested when they exist). A preview lists every Java setting as
  imported, not usable here (invalid values; Java window positions) or left out (features not in
  this client yet, accounts and credentials), with the engine list. Imported: record settings
  (load komi, go to last move, replace prompt), language, stone sounds, 棋盘同步工具选项,
  同步时不播放落子声 and the 不再提示 of the 自建算力 pondering notice, the editing settings
  (double click, drag, right-click menu or back, new-board prompt, markup tools, 其他 board
  size, no-capture), recent records, last
  folder, the engine list with its default and start-up choice, each engine's thread policy and
  benchmark results and its GTP configuration profile, analysis limits and interval, fast engine
  change, limit notice, 自动检测引擎是否存活, KataGo rules, the start-up benchmark choice, the network proxy and the
  board display settings (coordinates, move numbers including the older `show-move-number` keys,
  move-rank marks and judgement, candidate rows and options, next-move hints, sub-board mode,
  pure-network ownership style), the analysis-information settings above (graph, Kata形势,
  scoring rules, AI match, 独立选点列表 options, 记录选点信息到评论中; Java's window and column
  positions are reported as not usable), 自动棋谱快析, 精析设置 (with its 远程引擎 and
  `analyze-update-interval-centisecssh`), 局部精析's range, the
  lightweight model choice, 整盘精析 depth (`ui.whole-game-analysis-deep-visits`) and the 问题手
  settings, the 人机对弈 dialog values, AI time, random start and 后台思考 (`new-game-handicap` is
  Java's list position), the 引擎对战 dialog and 引擎对局设置 values (`engine-pk-*`,
  `new-engine-game-*`, `engine-game-match-rules`, per-side time modes and resignation thresholds)
  and the 自动落子 and 引擎对战 toolbar values in `toolbar-parameter`, and the AI 陪练 settings
  (`human-sl-training-last-settings`; Java's retired `LIVE_CORRECTION` becomes 边下边分析),
  HumanSL model path (`katago-human-sl-model-path`), 后台 HumanSL for 棋力评估
  (`strength-humansl-background-enabled`; the models in Java's `strength-models/` folder are
  copied into this client's, once, keeping their names), the 死活题 choices (`tsume-go-to-play`,
  `tsume-go-attaker`, `tsume-go-ko-threat`, `tsume-go-wall-distance`; an unknown choice acts like
  Java's 继承 / 自动检测 / 无), the 死活 toolbar button (`show-tsume-go-menu`) and the 抓取死活题
  thresholds (`capture-*`, the gray offset under Java's `capture-gray-percent`) — only values Java
  itself accepts.
  Java's own themes also come over: every folder of Java's `theme/` (in the Java folder, else the
  Java program folder) that holds a `theme.txt` becomes a theme of this client with the same
  name, together with the board, background and stone images it shows, found as Java finds them
  (relative to that theme's folder, or absolute) and copied into the theme. A TIFF picture, which
  Java reads but this client's window cannot show, is converted to a PNG of the same name
  (`board.tif` becomes `board.png`; a different picture already using that name in the theme gets
  the next free name, such as `board-2.png`). An image the theme
  itself lacks follows Java's fallback: the default theme's image setting (the theme keeps
  following it), else the file of that name in `theme/`, else the built-in picture. The theme
  Java had selected stays selected, and Java's translated 默认 / Default entry selects the default
  theme. An untouched copy of a bundled theme is skipped; a changed one becomes the user's copy
  of that bundled theme, whose own files are not changed. A theme folder that cannot be read,
  whose TIFF picture cannot be decoded, or whose `theme.txt` holds values no theme can use (such
  as a colour channel above 255), is listed as not usable with those keys or that picture and is
  not imported; the other themes and settings still come over. `default` is
  this client's default theme, so a Java theme folder named `default` comes over as
  "default (LizzieYzy Next)" (and stays selected if Java used it). Replacing
  a theme of this client with the same name is listed with the theme names and needs the same
  confirmation as other settings; the folders are written in the same change as the settings,
  so a refused or failed import leaves this client's themes as they were.
  Automatic-analysis conditions (`analyze-all-branch`, all ten `auto-ana-diff-*` keys,
  `exit-auto-analyze-by-pause`, `exit-auto-analyze-tip`) and `toolbar-parameter` slots 0–8,
  47 and 48 also migrate with Java's restore rules: an empty start becomes 1, empty time 2
  seconds, other empty limits stay empty, 按时间 is on as Java always shows it, and the visit
  limits, auto-save and sides follow their saved choices.
  Java's marker for its own bundled KataGo package is reported as not
  applicable. Engines keep using the Java engine and
  model files: relative paths are resolved as Java launches them (a bare program name is looked
  up on `PATH`; relative files, including a Java engine's `-jar` file, in the Java folder or an
  optional Java program folder), and missing programs or `-model`/`-config` files are listed
  without starting anything. Remote (SSH) engines keep their command as written (their files
  are on the host) and need a key file that exists when they log in with a key; a default or
  last engine choice that points to a left-out engine is left out too. An engine whose command
  holds a credential, or a remote engine with a saved password, is imported only after ticking
  "复制凭据"; 精析设置's remote password has its own box. Java's encrypted passwords go to the
  credential store as above, and credential values never appear in the preview, report or logs.
  Cloud provider settings, launch arguments, cached model catalog and custom links also migrate.
  Remembered cloud token and password each have a separate, initially unchecked consent box;
  source credentials are read only for a consented apply, not preview. Credential-bearing custom
  links/arguments follow the engine's credential consent. A failed destination settings write
  rolls back staged credentials; an unavailable credential store keeps approved secrets only
  for this session and reports that limitation. The source Java credential store is never changed.
  The 快速启动 switch and program list migrate too; a list whose commands carry credentials
  comes over only after ticking its own box, and importing never starts a program.
  Once this client has its own settings, every setting the import would change is
  listed and replaced only after confirming. Importing writes this client's `config.json` and
  `persist.json` together (a failed write leaves both unchanged; an existing settings file that
  cannot be read is not replaced), restarts the engines with the imported choice, and shows the
  report; the language applies after a restart. Java's files are never written, so both programs
  can be used side by side.
- An analysis time limit of 0 seconds means 9999 minutes, whether it is typed into the top
  toolbar's 限制: 时间 field, set in 综合设置 or read again after a restart; the saved value stays 0
  (Java stopped the analysis at once when 0 was typed into the toolbar, but read the same 0 as 9999
  minutes after a restart).
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
  focus is not supported by these releases.
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
  reports can be reviewed, applied and restored (a running engine, the 双引擎模式 secondary one
  included, starts again with the result and keeps its seat); they and Apple Silicon tuning apply only while
  the content of the engine, model and config still matches, which is checked before the engine
  starts without blocking the window. Apple Silicon deep tuning is experimental and asks first.
- 跑谱贡献 (KataGo distributed training, the 跑谱贡献 menu; Settings > 显示跑谱贡献菜单 hides it):
  KataGo跑谱贡献设置 keeps Java's fields (engine path, optional config, custom command with the
  Java SSH remote, user name, password, simultaneous games, ownership, no rating games, auto save)
  and 已配置引擎… fills the path from a saved engine. The line under the fields says what this
  client knows: an official package it installed shows its catalog status (a package marked
  unsupported, the Windows DirectML and OpenVINO builds, cannot be started; the others are not
  verified by this client against the training server); any other engine or command is unknown
  and still allowed. 开始跑谱贡献! starts `katago contribute` with Java's command line (the
  password is masked wherever the command is shown) after closing the analysis engines and
  opening a new empty board. The KataGo分布式训练 window shows the console, the tip, the watched
  game (type, komi, result, rules) and Java's game/move navigation, auto play, 暂停/恢复,
  顺序结束(延迟), 强制结束(立刻) and 保存所有棋谱. While it runs the board cannot be edited and
  other engine work (including 远程算力 and applying a settings import) is refused; while paused
  both are free again. A record opened while paused is left alone: the watch's auto play and later
  moves of the watched game only reach the contribution's own board, and choosing the game in the
  list shows it again. 跑谱贡献 does not start while a 试下 or a GMA move holds the engine. A
  crash restarts it up to three times (a missing CA bundle adds KataGo's `cacert.pem` once); a
  build without distributed support or one the server rejects stops at once with the engine
  failure window and 重启引擎.
  A KataGo older than the server accepts is stopped with a link to the releases page. Closing the
  application stops the contribution process.
- Fox AI direct play: 同步 > 野狐 AI 直连 opens a modeless session window. It listens on
  `127.0.0.1` (port 7000 by default) for the Fox AI 1.05 protocol. Use an authorized Fox AI
  account and a compatible official client; the application does not obtain account permission.
  Start confirms that moves will be sent automatically. When a record is open, choose to save,
  replace it, or cancel; saving ends that start attempt. The current record remains until a
  complete external game state is available.
  The dialog reads the foreground KataGo's rules and limits before starting. Set the per-move
  time and visit limits there; 0 means unlimited, but the external turn clock still bounds each
  search and submission. The foreground engine must support cancellable search, parameter
  readback and the required time controls. It is reserved while the live session uses it.
  Sent moves remain pending until the peer confirms them; only confirmed moves and passes enter
  the record. A missing confirmation stops automatic play rather than resending the move.
  You can browse the confirmed history and use 返回实战 to return to the live position; browsing
  does not change the engine's live position or save live candidates into historical nodes.
  While the session owns the engine, toolbar 激进度 edits use stop/set/readback. A successful
  readback saves the confirmed PDA preference; rejected or failed edits restore the last confirmed
  toolbar value. Java settings import includes this preference. 广度 is unavailable. Closing the
  dialog leaves the session running. A normal game end restores and releases the engine while
  retaining the listener for another game. Use 停止 to also close the listener. Restoration confirms
  the original rules, limits, no-clock state and displayed position before returning the engine;
  failure leaves it unavailable until restarted. An engine that ends while the session holds it
  is not restarted automatically; stop the session and restart it. Reconnection resumes
  automatically only when the confirmed history and engine identity establish a safe
  continuation; scoring and resignation remain actions in the official client.
  The real Linux application and KataGo have been exercised with a controlled local peer,
  including confirmation, reconnect, browsing, PDA and stop/restoration. Authorized official
  account/client interoperability has not yet been verified; controlled-peer checks do not
  establish it. This TCP integration is separate from screen-reading helpers. While a session owns
  the record, neither boards the 棋盘同步工具 reads nor a running 在线棋谱／弈客 follow change it.
- 在线棋谱 and 弈客直播 (同步 menu, the bottom bar's 同步 popup, function search):
  - 文件 > 打开在线链接 (`Q`) stops any follow, fills in a link from the clipboard and follows
    what the link names, like Java: 弈客 new live rooms (`#/live/new-room/…`, read every 刷新
    seconds from the signed detail service), older 弈客 live rooms and game rooms (the 弈客
    Socket.IO room), a 弈客 live board (read once), 野狐分享 and 腾讯欢乐 broadcast links (the
    huanle WebSocket), and any other link, which is read once and opened as an SGF record.
    The 弈客 kinds are recognized only on `https://yikeweiqi.com` and its subdomains, on the
    default port; the same link shape on another host, over `http` or on another port is any
    other link. Java recognized them on every host and read that host's `api.` server.
    中断 stops the follow, empties the link and closes the dialog; closing the dialog leaves the
    follow running. 新建 / 清空棋盘 and anything else that starts an empty board (人机对弈, AI
    陪练, 跑谱贡献) also ends the follow, as in Java; an engine match keeps it. An unknown link
    shows 无效的链接.
  - 弈客直播 (`Shift+O`) opens 弈客直播中心: the 推荐 / 地方 / 个人 lists with Java's columns,
    filtering, sorting, 刷新 / 更多, and 同步 (or double-click / Enter) to follow a room.
  - A followed source only writes the main line. Your variations, comments and marks stay. The
    cursor follows a new move only from the live end, on the first sync, or with
    同步 > 有新的一手时总是跳转到最新一手. Java replaced the whole game on every update, losing
    variations and moving the cursor; this client does not. A finished game adds its result to
    the last move's comment.
  - 打开网页 / 弈客网页版 open a 弈客-only web window with Java's toolbar (后退, address, 加载,
    停止同步). 加载 (or Enter) of a 弈客 room address follows it, and from then on a 弈客 room
    address the page moves to is followed automatically; any other address shows URL 无法识别.
    The 棋盘同步工具 helper's 弈客 signals act on this window as in Java: its start follows the
    room the page shows (reloading it) unless it is followed already, and its stop ends the sync.
    The window title shows the follow's status. Only `https://yikeweiqi.com` and its
    subdomains load in the window, plus the frames of the 腾讯验证码 the 弈客 login shows; other
    links go to the system browser. The page can call exactly one bridge command, which reports
    its address and 对局室 replies. The client checks these reports again before using them.
    Following a 对局室 (`#/unite/…`) reads the page's own signed-in requests, so it needs a 弈客
    login in that window.
  - Reads use Settings > 网络代理 like Java; the 弈客 Socket.IO room connects directly, as in
    Java. An unusable manual proxy shows Java's 网络代理设置 warning and stops the polling or the
    broadcast connection.
  - When the 弈客 Socket.IO server disconnects or refuses the room, the follow stops as it does
    when the server ends the room, and the record is kept (Java kept showing the room as followed
    with no more updates); a dropped connection still reconnects. A 腾讯欢乐 broadcast or record
    read that the server answers with an HTTP error status fails even when its body could be read
    (Java took the error body as the broadcast or the record).
  - Commentator variations are added as side branches with their own colours, passes and
    comments. A game that arrives while a borrowed engine is being given back waits for it; 中断
    meanwhile drops it.
  Verified on Linux (X11, WebKitGTK) against the live 弈客 service: the 直播中心 list, following
  a live room while browsing, adding a variation and analysing, stopping, 打开在线链接, the web
  window's 加载 auto-follow, logging in, and following a 对局室 (new moves, a variation of your own
  kept, moving to another room in the page, 停止同步). Not run live: the 弈客 Socket.IO rooms
  (the server refused connections during the run) and the huanle broadcasts (session tests with
  constructed frames).

Every other Java menu item, toolbar button and shortcut is present in its Java place but marked
as not yet available (greyed in menus; a notice when used).

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

The release-candidate packages (version 0.1.0, both identities, every platform below) were built
from one commit and installed on real systems. A second set, identical apart from version 0.2.0,
served as the newer version. The packages of the normal identity carry no update key yet, so their
in-program update is not available; the in-program update routes were checked with
acceptance-identity packages signed with a separate test key and a local release source.

- Windows 11 x64, on an isolated desktop next to an installed LizzieYzy Next, whose settings
  stayed unchanged: the normal installer installed for the current user, opened a record, ran
  next to the acceptance identity, was upgraded in place by the newer installer with settings
  kept, and was uninstalled with settings kept; the normal portable zip was extracted and
  started. The installer and the portable zip updated themselves from within
  the program; a wrong signature, a cancelled download, an installer that would not start, an
  unwritable or busy portable folder and an interrupted replacement each left the installation
  as it was, with a message. The uninstaller keeps the installer's remembered install folder
  (`HKCU\Software\qiyi71w\<product>`) as well as the settings.
- macOS 27 on an Apple Silicon Mac: the normal app from the Apple Silicon dmg (ad-hoc signed;
  Gatekeeper's assessment rejects it as not notarized) was copied to *Applications*, opened a
  record, ran next to the acceptance app, was replaced by the newer app and removed, keeping its
  settings both times. The Intel app was started only under Rosetta. The app updated itself in
  place; a package with a wrong signature and one that could not be unpacked left it unchanged.
  The Gatekeeper first-open steps in [docs/installing.md](docs/installing.md) were checked on an
  earlier build with the same signing.
- Ubuntu 24.04 GNOME, Wayland (and one X11 session): the normal deb was installed with `apt`,
  started from the application menu and opened records, ran next to the acceptance deb, was
  upgraded in place and removed, keeping its settings; the normal AppImage ran through FUSE and
  was replaced by the newer file. The AppImage updated itself; the deb update went through GNOME
  Shell's own authorization prompt, both dismissed (nothing installed) and authorized.
- Ubuntu 26.04 in a container under a virtual X display (not a GNOME desktop): the debs of both
  identities were installed side by side, the normal deb upgraded and removed with its settings
  kept, the normal AppImage run; the deb update was checked with authorization, refusals,
  package-manager failures and interruption.

Not verified yet: Windows 10, macOS 15, Intel Macs, Ubuntu 26.04 GNOME, the Windows SmartScreen
prompt, and full input-method coverage on Windows and macOS.

## Layout

| Path | Contents |
| --- | --- |
| `crates/lizzie-core` | Platform-independent core: game record model, SGF/GIB readers and SGF writer, browsing, settings, the LizzieYzy Next settings reader (`java_import`), the engine runtime (GTP framing, launch, analysis requests, rules, the foreground lease, KataGo analysis processes, thread policy and benchmark), automatic quick analysis, engine resources (`resources`: trusted package/model catalog, machine detection, recommendation, downloads, installs, verification), online sources (`online`: link classification, the 弈客 REST client and the sans-IO 弈客 Socket.IO and huanle sessions; the workbench's follow flow writes their main line), the credential sanitizer and the workbench command/view contract. No Tauri or DOM dependency. |
| `src-tauri` | Desktop shell: window, file dialogs, drag and drop, engine and helper processes, HTTP downloads, logging, diagnostics export, the 抓取死活题 screen picture (`screen.rs`) and overlay, the Web 旁观 page and board-stream servers (`web_board.rs`, Java's viewer page in `src-tauri/web`), the online transports (`online.rs`: GETs and WebSockets for the follow flow), the 弈客 windows and the remote page's one-command bridge (`yike_browser.rs`, `capabilities/yike-page.json`), and the optional acceptance channel. |
| `ui` | Web UI: menus, toolbars, canvas painters, dialogs and the Java UI strings. |
| `fixtures` | Sample records used by tests and manual checks. |
| `tools` | Maintenance scripts. |

The core owns the open record. The UI only renders `WorkbenchView` projections and sends
`Command` values through one Tauri command (`dispatch_command`); menus, toolbar buttons,
shortcuts and automated acceptance all use the same commands. Views carry an increasing `seq`,
and an open runs in the background under a ticket so a late result from an older open is
dropped. Engine results carry the record, position and komi they were requested for; the core
drops any that no longer match and sends the engine only the moves that changed.

The desktop shell sends view updates in order: on Windows as WebView2 web messages carrying the
view's JSON, elsewhere through a Tauri IPC channel. Engine-driven
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
node tools/cargo-test.mjs           # the same tests, test binaries run four at a time
cargo clippy --workspace --all-targets
```

`tools/cargo-test.mjs` takes `cargo test`'s selection arguments (`-p lizzie-core`,
`--test panels`, `--features …`) and passes the arguments after `--` to every test binary. Each
binary runs in its own process from its package directory, as under `cargo test`; at most four run
at once (`--jobs N`), longest first by the durations recorded in `target/cargo-test-durations.json`.
Starting every binary together makes the timing-sensitive tests fail. It reports every failing
binary with its output and exits non-zero, like `cargo test --no-fail-fast`.

### Generated files

- `ui/src/contract/generated.ts` is generated from the Rust types. After changing them run
  `UPDATE_CONTRACT=1 cargo test -p lizzie-core --test contract`; the plain test fails while the
  file is out of date.
- `ui/src/i18n/javaStrings.json` holds the Java UI strings used by the UI, taken from the Java
  `l10n/DisplayStrings_*.properties` bundles. Regenerate it with
  `python3 tools/extract_java_strings.py <java-checkout>`; the script requires the checkout to be
  at the reference commit and lists keys that are shown literally.

### Commands, settings rows and function search

Function search reads the Java catalog from the core (`searchFunctions(query, language)` in
`ui/src/contract/client.ts`, resource keys in the core's order) and decides each result's
destination in `ui/src/search/targets.ts`. A feature makes its entries reachable by registering
them where its control lives:

- Actions and windows: an entry in the `COMMANDS` catalog (`ui/src/commands.ts`, extended by each
  feature module) under the Java action id. Search runs it for ACTION entries and for the Java
  openers listed in `targets.ts` (weights download, acceleration, komi, rules, …); a bottom-bar
  button's command does what the button does.
- Menu items: the item's `id` in `ui/src/chrome/javaMenus.ts` plus its `COMMANDS` entry. Search
  runs a WINDOW item's command (`menuItemCommand`); for the others it opens the menu path and
  focuses the item (`locateMenuItem`) without running it.
- Toolbar controls: `data-function="<Java action id>"` on the control (`at()` in
  `chrome/toolbar.ts`, `located` in `chrome/bottombar.ts`). A control marked `unavailable` or
  disabled does not count. Search clicks a WINDOW control and locates the others; a bottom-bar
  button not on the bar is located in its 更多 popup (`locateHiddenButton`).
- Settings rows: the section page (`SETTINGS_PAGES` in `ui/src/engine/settings.ts`) builds the
  row with `data-setting-target="<Java setting id>"`, and its module registers the ids with
  `registerSettingTargets(section, ids)` (`ui/src/engine/settingsTargets.ts`) when it loads, so
  they are searchable before 综合设置 is ever opened.

Three Java catalog entries are not offered and never listed (`WITHDRAWN` in
`crates/lizzie-core/src/function_search/mod.rs`): the two lizzieyzy.cn kifu-history search windows,
whose service is offline and which Java reaches only through search, and 设置 > 界面外观, which only
switches the Swing look and feel.

Two mappings live in `targets.ts` itself:

- `OPENERS`: the Java ids that `FunctionSearchController.activate` opens with its own owner
  method (weights download, acceleration, komi, rules, AI coach). Each still needs its product
  `COMMANDS` entry; without one it is unavailable.
- `CONTEXT_ITEMS`: the board context-menu keys (`RightClickMenu.*`, `RightClickMenu2.*`) that act
  in `ui/src/editing/input.ts`. The context menu has no registration of its own yet, so a change
  that makes another context item act (or stops one) must update this list in the same change.

An entry without its destination is shown as not yet available. The acceptance probe's
`functionSearch.entries` lists every catalog entry with its destination `kind` (`action`,
`menu`, `toolbar`, `setting`, `window`, `context`), whether the destination is `missing`, the
current `reason` and `registered` (the id has a `COMMANDS` entry), so a smoke check can require
registered commands to reach a real destination; `functionSearch.open` describes the open search
bar (query, rows, selection, focus) and `lastActivation` the last opened result.

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

These source builds, and the Rust-only rebuild below, keep the normal application identifier
`io.github.qiyi71w.lizzieyzy-tauri`, so started as they are they use the normal program's
settings and log folders and its credential-store entries. Run them only with isolated data. On
Linux, point `HOME` at a throwaway folder and leave `XDG_CONFIG_HOME` and `XDG_DATA_HOME` unset
(or point them into it too); this does not separate the desktop session's Secret Service
keyring, so where passwords or keys are involved use a session without a Secret Service or a
throwaway user account. On Windows and macOS `HOME` does not separate the settings folder
(Windows) or the Keychain (macOS): use the acceptance-identity package from
`node tools/package.mjs --identity acceptance` (see [Packages](#packages)), which has its own
identifier, data folders and credential entries.

After changing only Rust code, `cargo build --bins --features acceptance,tauri/custom-protocol`
(the command `tauri build` runs) produces the same `target/debug/lizzieyzy-tauri`. It skips
`npm run build`; that step rewrites `dist/` and so forces the app crate to recompile even when
the frontend is unchanged. After changing the frontend, use `npx tauri build`, which runs `tsc`
itself as part of `npm run build`.

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

Both Windows packages carry the board synchronization helper
([qiyi71w/readboard](https://github.com/qiyi71w/readboard) v3.2.0,
`readboard-webview2-v3.2.0.zip`) as a `readboard/` directory next to the program. The script
downloads the release asset once into `target/readboard/`, checks it against the size and
SHA-256 pinned in `tools/package.mjs` and records it in the manifest (`bundled.readboard`).

`--identity acceptance` builds the same program as *LizzieYzy Tauri Acceptance*: identifier
`io.github.qiyi71w.lizzieyzy-tauri.acceptance`, binary `lizzieyzy-tauri-acceptance`, its own
install location, package name and data directories, and the acceptance channel compiled in
(`src-tauri/tauri.acceptance.conf.json`). It installs next to the normal program without
sharing anything with it. Packages of the normal identity never contain the channel.

The installers and the macOS app carry no paid code-signing certificate: the Windows files are
unsigned and the macOS app is ad-hoc signed and not notarized. See
[docs/installing.md](docs/installing.md) for what users see on first start.

Updates from within the program need packages signed with the project's update key:
`--update-pubkey` builds the public key in, and with the Tauri CLI's private key variables set
the script signs the NSIS installer, the portable zip, a macOS `.app.tar.gz`, the AppImage and the
deb and writes a `.update.json` per platform; `tools/update-manifest.mjs` combines them into the
release's `lizzieyzy-tauri-update.json`. See [docs/updates.md](docs/updates.md).

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
files and never overwritten), `quick-analysis-models/` for the lightweight model, `runtime/nvidia/`
on Windows, `downloads/` for package archives; an
interrupted download stays next to its target as `<name>.part` and resumes from there); "打开程序目录"
opens it. Strength models imported for 棋力评估 live in `strength-models/` and HumanSL 40 policy
caches in `strength-humansl40-policy-cache/`, next to `resources/`. AI 解说 keeps its ChatGPT
account list (no tokens) in `chatgpt/accounts.json` in the data directory; on Windows the keys it
remembers are DPAPI-encrypted under `secure-credentials/` in the configuration directory, while
macOS and Linux use the Keychain and Secret Service. LizzieYzy Next keeps its own
`config.txt` and `persist` elsewhere (for example `~/.lizzieyzy-next/`); this client never writes
there. The first start is detected by the missing `config.json`; declining the import offer
writes it. 跑谱贡献 runs KataGo in `contribution/` of the data directory (its downloaded models
and finished-game SGFs stay there) and 保存所有棋谱 writes to `ContributeGames/<yyyy-MM-dd-HH>/`
next to it. The training password is kept in the system credential storage like the keys above
(for the current run only where none is available) and never in `config.json`; a `password=` value
typed into the custom command is kept there too and saved as `password=******`, which the next start
fills back in.

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
non-ONNX builds include upstream distributed support and are offered to 跑谱贡献 as not verified
by this client; DirectML and OpenVINO builds do not support it and cannot be started there.
Models remain the b11 network from katagotraining.org (KataGo Neural Network License) and the
two b10 networks from the KataGo v1.17.1 release; the HumanSL network `b18c384nbt-humanv0` from
katagotraining.org's extra models is pinned for AI 陪练 only. License references are shown per asset; official
packages do not need the `licenses/` folder or `source-package.json` that LizzieYzy Next packages carry.

## License

GPL-3.0-only (see `LICENSE.txt`). The images, stone sounds and the winrate font in `ui/public` are copied
unchanged from LizzieYzy Next (GPL-3.0), and the application icons are generated from its icon;
see `ui/public/assets/SOURCE.txt` and `ui/public/fonts/SOURCE.txt`. The strength models and the
HumanSL 40 policy configuration in `crates/lizzie-core/resources/strength/` are copied unchanged
from LizzieYzy Next's `src/main/resources/models/strength/` (GPL-3.0). The Web 旁观 viewer page
(`src-tauri/web/index.html`, `board.js`, `board.css`) is copied unchanged from LizzieYzy Next's
`src/main/resources/web/` (GPL-3.0).
