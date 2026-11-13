# Changelog

What is new in each release of Artoo. A release's notes on GitHub are its section here, then its commits
(`.github/release-notes.sh`); a release tag whose version has no section here does not build. Write what changed
for someone using the app, not for someone reading the code; the commits follow anyway.

## 2.0.0-rc.3 — 2026-11-13

- **The Inbox needs no GitLab.** Its news comes from plugins and agents too, so it opens without a token, a remote
  or git, and lists what they bring at once; with nothing from any of them it says `Nothing needs you`.
- **Plugins: a page's own `⋯` rows, and ⌘S in a plugin's card.** A plugin page's `⋯` holds its own rows (Copy key,
  Copy branch name) though no button folds into it; ⌘S reaches a card a plugin shows over the window (New task keeps
  a draft) unless a theme is being edited.
- **Plugins: a space's own keys, its ⌘K Here, its page bar, its Inbox rows' facts.** A plugin binds bare letters to
  its own space (`space == tasks` in the key's context), where they win over the app's; its pages say what ⌘K lists
  under `Here` before anything is typed; its page bar holds its own path, view switch, filter (⌘F, ↑ ↓ ↵ in it) and
  buttons, folding into `⋯` as the app's do; and a row it lists in the Inbox shows its item's id, where it is, its facts labelled and
  `Open PAY-412`, with Done. Settings → Accounts heads the hosts `Code hosts`, for a plugin's trackers to follow.
- **MCP: a plugin's own tools.** A plugin gives agents tools of its own — a tracker's plugin reads an issue whole, or
  searches — and Artoo's MCP server lists them beside its own while Artoo runs with the plugin on; the agent's call
  runs in Artoo, in the plugin. A plugin that stops as it installs takes its tools with it, and one turned off in
  Settings takes them at once, the agent told its list changed. A tool's name is the plugin's namespace and a name
  (`jira.issue`), which the agent calls as `jira_issue`.
- **Agents: a note for an agent on what a plugin shows, not only on a line.** A plugin declares kinds of place — an
  issue's paragraph, a board as it is filtered — and a note written there waits, works and finishes in the Agents
  window beside the others, read by the plugin's words (`PAY-412 · description · ¶4`). The agent gets the plugin's
  context for it and the name of the plugin's tool that reads more; a click opens the plugin's page. The Agents
  window is in a plugin's space too, so a note added there opens it beside the page.
- **A plain folder and a repository with no remote: every page opens and says what is missing.** The menu no longer
  greys Branches and Merge requests or writes under them; each opens like any page. A plain folder's say `Not a git
  repository`, `notes has no .git folder`; a repository with no remote says `No remote` once, on Merge requests, with
  what they need it for. The bar over such a page is its name alone.
- **Branches: a Viewed box on every file, kept while an agent commits.** A commit's, a branch's and the working
  tree's files each have a `☐ Viewed` box on their card instead of Mark reviewed; a tick folds the card to its head,
  and the list over them counts `1 of 3 viewed` (⌥⇧V still ticks them all). A tick stays while commits leave the file
  alone; when one changes it, the file unticks itself and carries `↻`, `Changed since you ticked it`. Each commit row
  says how much of it you viewed — `1 of 3`, `✓ viewed` — counting a change you ticked before it was committed. Folders
  carry no box, `Filter by path` shows over three files, a click on a commit's id copies the whole of it, `Message ▾`
  says `4 more lines ▾`, and `231a547 · against its parent` and a branch's `Since merge base · Direct · Swap` went.
  A merge request's own diff keeps Mark reviewed. The branch checked out has one page, the Working tree: its commits
  over every file it changed since main, committed or not, `Checked out feature/login-state` first in its facts.
- **Code reads any revision, and says which once; main's page is laid out like a branch's.** The chip over Code's
  tree, `working tree ▾`, drops a list under `Filter, or a commit id`: the working tree, HEAD, the branches, origin's
  and every tag, highest version first, in a list that scrolls; what you type filters it, and any id or `HEAD~3` is
  offered with its message. At another revision the chip is tinted, `release/2.4 ×`, and × goes back to the
  working tree; a file's head says only `19 lines`, without `read only` or the revision again. A commit's and a
  branch's menus gain `Browse files here`. main's page offers a quiet `Compare… ⌥⌘C` instead of Create merge request,
  has the tabs `Commits 10 · Changes 3` (⌃⇥ turns them over) with the commits running to the column's foot, and a row
  `Remote origin · in sync · Latest 231a547 · 25 d ago`; a section's key is a keycap, `⌃2`. A run of commits is
  called `2 commits` in the path, and a branch's remote reads `origin · in sync`.
- **Code goes from a name to where it is declared, to a line, and says where you are.** ⌘-click a name, or put the
  caret on it and press F12: a card opens under the line, `By name · 3`, the declaration in this file first and then
  the others, `LoginState.kt:3` with its line and folder, the name underlined in the accent; ↑ ↓ ↵ open one in the
  preview tab, ⌘↵ in a tab of its own, ⇧F12 searches the word in the repository. A card opened at the window's foot
  scrolls into view. Holding ⌘ underlines a name under the pointer that is declared somewhere. A name declared once
  elsewhere opens straight away. There is no language server: the repository's declarations are read by their keyword
  (`fun`, `class`, `enum class`, `struct` …) the first time you ask, and a file you or an agent changed since is read
  again at the next ask — Java's methods, which no keyword declares, are not found. ⌃G (or a click on the file's
  `N lines`) puts `Line` in the file's head; ↵ goes there, `42:7` to its seventh character with the caret. Over the code, a line names the declarations you are
  inside — `class LoginViewModel › fun submit` —, and a click goes to one.
- **Code finds in the file you read, and the repository has a search.** ⌘F on a file in Code opens a find bar under
  its head — `amount`, `.*`, `1 of 2`, ↑ ↓ × — with the hits tinted in the lines and the current one filled. A minimap
  at the code's right draws the file small, each run of characters in its colours, marks the find's hits where they
  stand in their lines and the lines that differ from HEAD (they have a bar
  beside their numbers too), and shows what is on screen as you point at it; a click or a drag scrolls there. ⌥⌘F
  opens `Search in payments-service`, a window on the right with its icon first on the stripe: every file of the
  repository, `capture · 37 in 4 files · working tree`, the lines grouped by file — a file's head folds them —, each
  in its colours with every hit on it marked. ↑ ↓ ↵ or a click open a line in the diff when it changes that file, else
  in Code at its line, in the preview tab, and the window stays for the next one; closed and opened again, it has your words and what it
  found. It reads nothing as it opens and searches while you type, in the background, hits coming in as they are found
  (`Searching… 240 of 587 files`); typing on narrows the last search, so a large repository stays quick.
- **A setting changes where it annoys you.** Right-click a button in the title bar — a plugin's too — and `Hide` it;
  Settings → Appearance lists what is hidden with `Show all`. Right-click the fetch status for `Retry` and `Stop
  fetching this repository`. Settings asks less: generated files always start folded — those the repository's
  `.gitattributes` marks `linguist-generated` too, and not those it marks `-linguist-generated` — and tabs are as wide
  as each project's `.editorconfig` says, with no option for either; the projects inside the menu of spaces went (Rail and
  Tabs stay, and a Menu kept opens as Tabs); About gains `Open settings folder`. A `Colour-blind` theme ships: One
  Dark with blue for added lines and orange for removed ones.
- **Themes are one row each, and plugins can bring them.** Settings → Appearance says `Theme  ● One Dark ▾` and
  `Code theme  ● Same as Theme ▾` instead of two walls of cards, so the fonts are on the first screen. Either row drops
  every theme in one list under it — `Filter 30 themes`, grouped `Built in 6 · From plugins 22 · Yours 2`, each with
  its swatch and the one in force checked; ↑↓ tries the theme on the whole window, ↵ keeps it, esc goes back to the one
  you had, and `Edit` sits on the row you are on. A code theme's contrast on the window's diff is a muted fact on its
  row (`2 under 4.5:1`) rather than red on the page. Your themes and the file icons fold into one `Your own · 2 themes
  · 4 file icons` row, and Set up's Look shows the same two rows. A plugin brings its themes as theme files
  (`artoo_ui_kit::theme::add_plugin_themes`); editing one makes a copy of your own.
- **Settings finds a setting by its words.** `Search settings` heads the left column (⌘F while Settings is open):
  typing lists the hits under their sections, each with its own switch or chips, and every section counts its hits;
  ⌘K lists settings too — `font` finds the faces, `notifications` what Notifications holds. The sections say what is
  in them — `Review · Appearance · Keyboard · Notifications · Projects · Accounts · Agents · Plugins · About` —:
  where files and repositories open moved to Projects, with this repository's own settings under its name, and Check
  for updates to About. Rows are 44 px in one 640 column, a row off its default has a dot and `Reset` on hover, `Done`
  sits at the foot of the left column, one editor found is said in plain words, and `Format comments as markdown`
  left (its key, ⌥M, stays).
- **A setting found is its own row, with what found it marked.** Search settings shows every setting live where it can —
  `dark` the Theme row, `claude` Claude Code's, `notif` System notifications and the Dock's count — with the words you
  typed marked and, when a row was found by a word it does not show, that word under it (`dark`, `In Notifications`).
  A setting picked in ⌘K lights its row. Settings walks by keys: ↑ ↓ change the section, ⇥ rings a row, space flips
  it, ← → step its choice.
- **A failed fetch reads as a fact.** The title bar says `Can't reach origin · fetched 14:32` beside an amber dot, the
  remote by its own name (the one git could not reach, of several), with `Retry` as its own button, instead of a red
  sentence; its tooltip names the host, the likely cause and the last fetch. Sentences that described a page went — how
  Artoo signs in, the faces, Raw / Rich's key, Merge's warning, the Agents setup line, the Inbox preview repeating its
  row — and the layout's fold widths moved to the interface size's tooltip. A row's tooltip says what the row does not —
  `Juniper Hale · feature/login-state → main`, `updated Thu 15 Oct 2026, 16:25` — and the title or a part only when the
  row cut it, in the merge requests list, a request's commits, its versions and the Inbox. Every tooltip now opens after
  the same 800 ms, and the merge card's × is named.
- **The Agents window is grouped by what you owe.** `Needs you`, `Working` and `Done` replace the filters; a row is the
  agent's own last words — `One sealed class, or a sealed class and a separate error type?` — with where and how long
  ago under them — a comment of yours says how long ago from when you wrote it —, and one button for the one thing to do
  (`Answer`, `Send`); the rest is in its right click. An answer you picked and have not sent yet is said under the
  question: `Your answer: From the toast`. The header names the chat your notes go to. The stripe counts what needs you,
  a tab behind another takes its agents' answers too — the rail counts them and `Claude asks you` arrives as a
  notification —, and Settings → Agents says which profiles are connected (`~/.claude connected · ~/.claude-work not
  yet`), showing the command by hand only when connecting failed.
- **The composer opens public on a merge request.** A comment on a line of a request's diff starts orange, for everyone
  on it — `To: Everyone on !398` over the text, `Comment on this line`, and `Add to review` beside a filled `Comment`. ⇥
  turns it into a note for the chat working on this branch and back; the chip's ▾ lists everyone on the request and
  every agent chat of the project — its letter on its hue, its name over its branch, whether it listens — and the note
  goes to the chat picked. A reply says who reads it the same way, `To: Everyone on !398` over its text, and ⇥ turns it
  into a note for your agent. ⇧⌘↵ in a reply keeps it pending with your review instead of posting it, and a pending
  comment can be edited before the review is sent.
- **Discussions count the work left.** The window says `Discussions · 3 open`, each row ends with who moves — `waiting
  on you` or `waiting on Ember` —, pending comments come first and resolved ones fold under `Resolved · 4`; its icon
  counts open and pending, and ⇧N walks only the open ones. A general comment opens whole in the window, every note,
  with Reply… and Resolve under it, and the reply is written right there. Its foot is a field, `Comment on !398 ⌘↵`,
  that writes a comment on the whole request; ⌘↵ with nothing being typed opens it. A pending comment's right click and
  ⌘K offer Send now; Reply has its own ▾, `Reply and resolve ⌥⌘↵` or, on a resolved discussion, `Reply and reopen`. A
  comment to your agent goes away by its state: a draft is deleted with Undo, a sent one withdrawn, one the agent read
  archived, under an archive icon — never erased. A comment changed after it was written says so: `20 min ago · edited`.
  Edit and Delete on your own note sit in its hover bar beside ☺+.
- **The diff keeps its room.** The narrow steps measure the diff, not only the window: when it would fall under 480
  px the side panel folds into the menu's column first, and only then does a tool window float. The room is measured
  as the window lays it out, so with projects as tabs, or in zen, the side panel stays where it fits. At 1 100 px with
  Discussions open that is about 590 px of diff instead of 305. The right stripe holds the windows you work in —
  Discussions, Agents, Outline —; Report a Problem… is in Help, ⌘K and ⌥⌘/, and the Inbox has no stripe at all.
- **Back goes where you jumped.** A file picked, ⌘P or a line gone to is a stop of its own, so ⌘[ after a jump in a long
  review scrolls back to where you were instead of leaving the request; scrolling is not a stop. ‹ says where it goes —
  `Back to !415` — and holding it or its right click lists the last places, a place by its file and line
  (`LoginViewModel.kt:46 · place`). ⌘E lists what is open first, the latest first (`!398 … 3 of 8 reviewed`) with the
  menu's dots for drafts and news, then the places you chose, and opens on the second row, so ⌘E ↵ returns you. A
  request only looked at is one italic row in the menu that the next one replaces; marking a file, a comment or an
  approval keeps it. An open row's right click closes it, the others, all of them or the merged ones.
- **Hold ⌘ to see the keys.** Held a moment, ⌘ shows each repository's ⌘1…, and ⌘B ⌘[ ⌘] on the title bar's buttons; ⌃⌘
  shows the spaces'. One of these done with the mouse says its key a moment at the window's foot — `Collapse the menu
  ⌘B` —, once for each key. Settings → Keyboard turns these helpers off. Settings → Keyboard reads by headings — Move,
  Comment, Find, View, Requests, App, each command under the one it says — with the menu's names, Change on the row you
  point at and on the one ↑ ↓ from the filter are on (↵ changes its key), and a clash says which command it shares the
  key with. Create merge request moves to ⇧⌘M, its key in its tooltip.
- **⌘K knows where you are.** Opened empty it lists `Here` — on a request `Approve this merge request`, `Submit review`,
  `Merge…`, `Copy link`, `Next merge request`, each with its key — then `Recent`, the places you last chose, a file with
  the request it was in and the line you left it at (`AuthApi.kt !398 · line 23`), each row with its icon. Approve,
  reply, resolve, edit, react and the other acts are in it now, and a command that matches what you typed comes first.
  The title bar's field reads `Search or run an action`, and ⌘K's own field `Search`.
- **The keyboard goes further in a request.** `c` with nothing typed stands on the change you are at — `Comment on line
  47`, ↵ writes there —, ⇧↑ ⇧↓ stretch it over lines, and `n` `p` (and F7) carry on into the next file. The discussion
  the keys act on wears a ring: a click or ⇧N gives it, esc lets it go, and `x` on it says `Resolved ·
  LoginViewModel.kt:47` with Undo — ⌘Z undoes the newest toast's. ⌥1 ⌥2 ⌥3 go to Overview, Commits and Changes. On the
  merge requests list ⌃⇥ steps All · To review · Mine, whose counts follow the filter — a merged or closed list reads
  on to its end while you filter, so `30+` becomes `75` —, and in the Inbox its kinds; the
  filter finds an author by username too and marks what it matched, the starts of words included (`juha` marks
  **Ju**niper **Ha**le). On Windows and Linux a tooltip spells a key as it is bound there: `Overview Alt+Shift+1 ·
  Ctrl+I`.
- **A request's Overview shows what is in the way.** `In the way of merging` lists each reason under the facts —
  `Approvals 1 of 2, one more needed`, `Pipeline lint failed  Pipeline →` — without opening Merge. The author and the
  branches are said once, beside the title; `Your review 0 of 8 files` replaces "Reviewed", and the facts end with
  `Commits 3`. The column beside the description adds Labels, Milestone and `Discussions 1 open · 1 resolved →`.
- **A new request can ask anyone in the project to review.** Reviewers lists everyone in the project by name as it
  is focused; type to find a member by name or username; ↵ adds them. The list opens over the field where there is no
  room under it. With nothing to merge, Create reads `Create · Nothing to merge` and cannot be pressed. ⌘K's
  Create merge request… opens the same form; the one-question-at-a-time card is gone.
- **The merge requests list says whose turn each one is.** Groups are `Waiting on you`, `Waiting on others`, `Yours` and
  `Done`, from what happened on the request: a comment hands it to the author, a push or a reply brings it back,
  approving ends it until the next push. Each row ends with the turn — `Waiting on you · 1 reply`, `Waiting on Juniper ·
  1 thread`, `You approved` — and its second line reads `!398 · Juniper Hale · ✗ lint · 1 of 2 · +120 −34 · 2 open · ⚠
  conflicts`, a target other than the main line last; a narrow list drops the least needed parts whole,
  measured in the face you chose. `New` marks a
  request you never opened, `Draft` a draft, which sorts last. The preview shows Size, Pipeline, Approvals, Reviewers,
  In the way, Discussions and Your review as labelled rows. `Display ▾` groups by author or not at all, and sorts by the
  oldest opened (`opened 12 d ago`) or by size. A request's facts are read a few at a time, the one you pick first, and
  kept for the next start. A reviewer is called what GitLab or GitHub calls them, not by their username, and a request
  whose branch you have not fetched still shows its size, as the host counts it.
- **A request's Changes is one bar, one toolbar row, then the files.** The title sits in the page bar after `!398`, with
  the failed job (`✗ lint`) beside Submit review; the row over the diff and the file row are gone. The current file's
  header holds `1 change ↑ ↓`, Open in Zed and Mark reviewed, and stays at the top while you scroll through it.
- **The description is a click away.** `Description ▾` after the title opens it over the diff, with the issue it is
  for above it (`PAY-123 ↗`). In Commits, a commit with a body shows its first line under the subject.
- **Merge says what is in the way.** `Merge · needs 1 approval`, `· lint failed` or `· conflicts`; more than one reads
  `Merge · 2 in the way`, and its list links each to its fix (`Pipeline →`).
- **Done reviewing sends what is pending.** When every file is reviewed, the card's green `Submit review · 3 ⇧⌘↵`
  sends your comments, with your approval when you may give it. `a` with comments pending opens Submit review on
  Approve instead of approving without them.
- **Coming back after a push shows what changed since you read.** A request you read before its last push opens on
  `Since v2`: only the files that push changed, each marked `↻ v2`, `3 files` beside the switch and `5 unchanged` at
  the foot of the list. `All · Since v2 · Commits` takes you to the whole request or one commit at a time, and `s`
  goes between All and Since. Versions tags v2 `you read`. The request's compare sides moved into View ▾.
- **The diff's toolbar and the page bar fold instead of cutting.** As the window narrows, Submit review keeps only its
  icon and Approve drops its key; Collapse all, then Split · Unified, go into View ▾.
- **A toast says who sent it, and you can hold it or turn it off.** It reads top to bottom: `GitLab · Ember Lind
  mentioned you`, the request under it, then `Open ⌥↵` and `⋯`; a plugin's starts with its name, `Jira · Assigned to
  you`. A bar shows the time left and stops while the pointer is on the toast. `⋯` turns that kind's toasts off —
  `Toasts for Mentions are off · Undo` — or opens Settings → Notifications. Toasts sit under the page bar, no longer
  over its buttons.
- **The Inbox says what happened, and from where.** Its filters are what happened — `All 7 · Mentions 1 · Review
  requests 1 · Asks you 1 · Failures 3` — whichever source it came from, and `Source: All ▾` lists every source, a
  plugin's too, each with what is new in it; type to find one. Each row names its source before its time
  (`GitLab · 6 min ago`), a failed pipeline reads `Pipeline failed` and an assignment `Juniper Hale assigned you`
  instead of both "has a to-do for you", and a row from GitHub no longer starts with a blank. New rows come first under
  `New 7`, then `Read`. ⌘F filters the Inbox by its words.
- **Nothing in a narrow window is cut off.** At 700 px the title bar keeps its search and its gear, its fetch status
  shortened to `Fetch failed · Retry`; what the page bar has no room for goes into its `⋯` — Merge, Open in GitLab,
  Copy link —, and `Approve 1/2` drops its key. Split is marked only while the diff is split: a diff under 700 px is
  unified, Split dimmed with its tip `Split needs 700 px`, and a diff that wide splits whatever the window. The menu's
  edge drags, from 216 to 360 px.
- **A list that could not be read says so.** It reads `Couldn't read the merge requests` instead of "No merge requests
  yet", under `Can't reach gitlab.example.com · list from 14:32`. An empty list is its state and its button —
  `Nothing to review`, `No mentions`, `No open merge requests` — without a sentence on what it is for; a filter that
  finds nothing says where it looked and clears with a click. Agents says `No chat listens here` with `Set up…`.

- **Cut text keeps what tells it apart.** A path is cut at its front, so its file name always shows
  (`…/viewmodel/impl/LoginViewModelImpl.kt`), and a name too long alone loses its middle; two branches that start
  alike keep their endings. Every cut ends in `…`, two-line text too, and F1 (or ⌘K's Show the whole text) opens the
  whole text of the row picked at the row, where ⌘C copies it. An agent's form options wrap instead of being
  clipped, and a plugin's crash reads whole.
- **Keys that no longer misfire.** ⌥ and a letter type a character in a comment instead of changing the diff; the
  composer's button shows ⌘↵; bookmarks are B and ⇧B, leaving F3 to macOS; ⌥⌘3, ⇧⌘N and ⌥⌘L are gone (⌃⌘1 is the
  Inbox, ⌘K leaves a merge request).
- **A file you read counts wherever you read it.** A file marked in Since v2, in a comparison of the same change or in
  every commit touching it counts in the merge request; a rebase that leaves a file's change alone keeps its mark,
  and a file changed by a push says the version you read it at, `↻ v2`.

- **Settings → Notifications.** Each kind of news — six from merge requests, four from agents, and a plugin's — has
  its own switches for the Inbox, a toast and the system's notifications. System notifications moved here from
  General, and say so when macOS has them off for Artoo, with a way to its settings. The Dock's icon counts what
  waits in the Inbox. A failed pipeline or a request that can't merge is its own kind, apart from other to-dos, and
  an agent asking you something now shows a toast. Plugins add their kinds, and their own rows in the Inbox.

- **Images open whole.** A click on an image in a merge request's description or comments, or on an image in a diff,
  opens it over the window with its name, size, who added it and when; ← → go through the page's other images,
  Download saves it to Downloads, Open in GitLab (or GitHub) shows it there, esc closes. Plugins open their images
  in the same viewer.

- **Try a plugin you are writing.** Settings → Plugins → Local takes a plugin's project folder or its library, with
  no repository or release. After each build, Restart Artoo runs what you built, and its row says why if it was not
  loaded.

## 2.0.0-rc.2 — 2026-11-09

- **Plugins can be built for Windows.** What a plugin is built against now holds everything a Windows build links
  with.

## 2.0.0-rc.1 — 2026-11-09

- **Plugins.** Artoo loads plugins as it starts: libraries built against its SDK. Settings → Plugins finds them on
  GitHub, installs, updates and removes them, says why one was not loaded, and turns one off for the next start. One built for another Artoo is refused before any of it runs, and one that fails as it installs leaves
  the app running. A plugin that stops Artoo is named at the next start, with a way to turn it off and to report
  it to its own repository; one that stops it twice as it starts is turned off.

- **GitLance is now Artoo.** The app, its window and its menus are Artoo's. The first start takes everything
  GitLance remembered — the hosts' tokens, the marks of what you reviewed, the settings, the keys, the pictures
  — from GitLance's folder, which stays as it was, so GitLance still opens beside it. Agents are connected
  again under the new name: the first start offers Set up's Agents, and Connect moves every Claude Code profile
  over, GitLance's entry and skill removed; Codex is given the command that does the same. Open chats need a
  restart to see Artoo's tools. macOS asks again before notifications and before a JetBrains IDE's token is
  copied, as it does for any new app.

- **A commit of the main line is a page of its own.** Picking one in the side panel opens it as a commit's page,
  as a commit of a branch with its own commits already was: Previous and Next in its bar, esc back to the branch
  (its newest commit), ⌘[ and ⌘] through the commits you looked at.

## 1.3.0 — 2026-10-29

- **Pin a tab in Code.** Pin tab — in a tab's right click, ⌘K or ⌥⌘P — keeps a file or a folder first among the
  tabs, a pin where its × was and a line after the last one. A preview is never put in its place, Close others,
  Close to the right and Close all leave it, ⌘W leaves it too, and it is there again after a restart. A click on
  its pin unpins it. Close to the right is new in a tab's right click.
- **What is beside a folder, from Code's path.** A click on a folder's name in the path over a file lists what
  is beside it — folders first, then files, the one you came through saying "here" — and typing filters it. ↑ ↓
  and ↵ open a folder's page or a file's tab; the first row opens the folder itself, as the click used to.
- **A comparison's commits.** Comparing two branches, tags or commits lists the commits it holds over its
  files, newest first. A click shows one commit's own changes, a ⇧-click a run of them, ↑ ↓ step through them;
  "a41f9c0 only" says so over the diff, and esc or its × bring the whole comparison back.
- **Merge, from the request's page.** Merge is beside Approve. While something stands in the way it is quiet and
  says how many — "3 in the way" —, and its ▾ lists each reason with where it is fixed: the approvals still
  needed, the discussions not resolved (they open), the pipeline that must pass (its page opens), a draft,
  conflicts. Rebase is offered when the branch is behind and GitLab needs it rebased, "Merge when the pipeline
  succeeds…" while the pipeline runs, and Cancel once that is set. Merge… opens a card: from → into, squash and
  delete the branch as the project has them (GitHub: merge commit, squash or rebase, as the repository allows),
  the commit message, and ⌘↵ merges. It sends the version you have, so a push nobody has seen is refused — the
  card says so and shows the new version. ⌘K has Merge this merge request.
- **Set up GitLance.** The first start opens a page of three tabs before the welcome: Agents connects Claude
  Code's profiles and gives Codex its command, Keys picks GitLance's, Android Studio's or VS Code's keys and shows
  the ones that matter, Look picks the theme, the code's colours, the panes, the faces and the sizes. Each is
  what Settings does, at its shortest; a tab is ticked once it is set. Later or Done brings the welcome back, and
  Help → Set Up GitLance… opens it again.
- **Fonts and size.** Settings → Appearance → Fonts and size picks the interface's face and the code's from
  every face installed, each drawn in its own type: a few in the menu, Other… for all of them, filtered as you
  type. The code's size is in points there, beside the interface size, which moved from Settings → Diff too. At a
  larger interface size the narrow layout comes sooner, as the text needs more room — at 115 % the menu folds
  under 1 472 px instead of 1 280 —, and a line under the rows says where.
- **Staged and not staged, apart.** The working tree's files are in two groups, Staged and Not staged, each under
  a header that folds it and says how many; the filter is over both. A file partly staged is in each with its
  own part's lines, and every file's card says which part it is. Not staged only shows that group alone.
- **Pictures in comments, kept.** An image in a comment or a merge request's description — a screenshot uploaded
  to GitLab, a picture from the web — is drawn under its words, and a click opens it; a document's web images and
  badges are drawn too, not only named. Each is fetched once and kept on disk with people's pictures, so it shows at
  once the next time, offline too; a private project's uploads are asked with your GitLab token.
- **A field keeps what you wrote, not the keyboard.** Click outside a comment, a filter or the find bar, press esc
  in one that holds text, or go to another file or page, and it keeps its text, dimmed, while the keys go back to
  the diff: V marks the file you are on, j and k move. Click in it, press `c` beside an unfinished comment, or come
  back to its page, and you go on typing. A file with an unfinished comment shows a pencil in the files list; a
  filter left with text clears with a second esc.
- **Links without the browser.** A merge request's right click — in the list, the Inbox, its row in the menu, its
  page — copies its link or its number; a branch's, a commit's and a comparison's copy their page on the host, a
  run of commits or two versions of a request their comparison there, and a file's in a request's diff its place
  there. ⌥⇧⌘C, or Copy link to this … in ⌘K, copies the page on screen's. A toast says what was copied; a branch
  or a commit not pushed yet says it has no link instead of copying a dead one.
- **Every cut text gives the whole of it.** A name, a title, a path or a line cut short shows all of it on a long
  hover, everywhere — only when it was cut.
- **The size of the whole change.** Over the files, a row says how many there are and their lines added and
  removed, in the column of each file's own; with a path typed, of the files shown. The review line says only
  the numbers: 4/8.
- **An agent's work leaves a dot.** What an agent puts for you — its changes opened, a merge request drafted,
  requests gathered, pending comments, notes — marks its row with an orange dot until you open it, and the
  Branches or Merge requests row while their rows are folded. A row it left comments on comes under Merge
  requests by itself.
- **The New merge request card is easier to correct.** From can be changed to another branch, keeping what is
  written; From's and Into's lists open under their field, over the form, and ↑ ↓ walk them (they moved the
  commits behind the card); esc closes the list, not the card. A list of labels or milestones keeps the wheel
  instead of scrolling the form with it. An agent's draft opens with its branch however you get there, not only
  from its toast.
- **An agent shows you its changes without being asked.** It opens them in GitLance, with a summary and a note on
  each file the diff alone does not explain, after a commit, after it makes or rewrites a branch (a squash, a
  rebase, a cherry-pick, a reworded commit), after it addresses your comments, and before it asks you to look or to
  approve — a local branch not pushed yet included. Settings → Agents → Connect also gives Claude Code a `gitlance`
  skill saying the same; press it again to add the skill to a profile connected before.
- **Every way in, easiest first.** A repository whose host GitLance has no token for lists the ways this computer
  offers, each with the account it signs in as and a button named for what it does: the GitLab or GitHub CLI's
  token (Use — nothing is asked, nothing copied), the one git keeps, one made with your SSH key (read-only under
  ▾), one copied from IntelliJ IDEA, Android Studio or another JetBrains IDE signed in to the host (Copy… — macOS
  asks once; an OAuth sign-in is not taken), or one made on the host's page and pasted. ↑ ↓ choose, ↵ connects,
  ⌘V pastes a token. A way found that cannot be used says why underneath — a token for cloning only, a key the
  host refused, a CLI signed in elsewhere. The way picked is remembered for the host; Settings → Merge requests
  lists the same ways, the one in use tagged, to change it.
- **A token found is no longer used without a word.** git's token and the GitHub CLI's were taken as they were
  found; they are now listed with whose they are, so a second account is not used by mistake. Once picked they
  are used as before.
- **The window never waits on the GitHub CLI.** Its token was asked for each time the window drew; it is read once
  a run, in the background, and again when GitHub refuses it.
- **A file marked reviewed stays marked.** Once 300 diffs had marks, marking a file in a new one could do nothing
  (V and its box alike) or wipe another one's marks; now the diff marked longest ago makes room, never the one
  being marked. A merge request's ✓ in the lists, and how far you got in it, no longer drop away the same way.
- **A pending reply sits under its discussion.** A reply written in review but not sent yet shows right under the
  discussion it answers, on that discussion's line, saying whose it answers — in the diff, in Pending comments
  and to an agent — instead of somewhere else or nowhere.
- **V reaches every file.** V goes on to the next file not reviewed in the files' own order, opening a folded
  folder to get there, and goes round to the first ones when it reaches the end.
- **An agent can point you at a place.** When it says where something is — a file and a line, a commit, a comment
  on a merge request — a toast says so, "Claude Code points at nav.rs:785 · Show", and Show takes you there: the
  line in the diff on screen when the file is in it, else in Code. A file opened at a line from a note now shows
  that line, not the file's top.
- **The whole history, not its newest 5000.** A branch's commits come a thousand at a time as you scroll down,
  the count saying 1000+ while there are more; a search reads on through older ones until it has looked at all of
  them, and a link to an old commit reads down to it.
- **News reaches you in the background.** Settings → General → System notifications (off unless you turn it
  on) sends what comes while GitLance is not in front to the system's notifications, on macOS, Windows and
  Linux: a mention, a review asked, a reply, a to-do; an agent's pending comments, its changes, its answers. One
  click brings GitLance forward with it open. Many at once come as one; nothing comes while GitLance is in front.
- **Mine and To review, merged or closed, end where yours end.** The host picks them, so the list stops at your
  last one instead of reading the whole project to find it, and All · To review · Mine say how many there are in
  all.

## 1.2.0 — 2026-10-27

- **The background fetch runs from the start.** A repository just opened showed Fetch in the background off and
  fetched nothing until it loaded again; it is on from the first moment now, as it has been meant to be since 1.0.1.
- **A merge request shows its own commits.** It was compared from its target branch as this clone last fetched it,
  so a request rebased onto newer commits showed every target commit since as its own, and one merged by a
  fast-forward showed no changes at all. It is now compared from the base and head GitLab or GitHub gives it; a
  commit of them that is not here yet is fetched once.
- **A comparison stays as you left it.** Its Swap, Direct and Use origin/… were undone when you came back to it
  from another space, and its side panel listed the commits and versions of the branch or merge request opened
  before it. It now comes back as it was, with only its own changes beside it; the working tree too.
- **The menu scrolls.** With many merge requests or branches open under it, the rows that did not fit were cut off
  at the window's bottom; the menu now scrolls, and brings the page on screen into view when you go to it.
- **v opens the file it moves to.** A generated or lock file it went to stayed folded, and the file it marked
  opened again when you had folded it; the next file now opens at its first lines, and the one marked stays as
  you left it unless View → Fold reviewed files folds it.
- **Send one pending comment now.** A comment kept for the review (⇧⌘↵) can go on its own, the others still
  pending: Send this now beside its Delete, and on its row in Discussions and Submit review. On GitLab; GitHub
  sends pending comments only with the review.
- **An agent can open GitLance.** Its MCP server's new `open_gitlance` starts GitLance on the agent's repository,
  or opens the repository in a tab behind the one you are on when GitLance runs without it, with a note saying
  which agent did. Agents are told to do it only when their work needs you to look.
- **An agent can review a merge request with you.** Through GitLance's MCP server it reads the request as GitLab
  or GitHub compares it and leaves pending comments on its lines, with GitLance's own token; a line not in the
  diff is refused. They show at once, marked with the agent's name, and a note offers Review, which opens the
  first. They stay pending, only yours to send, delete or submit.
- **An agent can gather the merge requests it reviewed.** They come in quietly under Merge requests, in its
  order, each with the agent's mark, how many of its comments wait there and its line on what it did; opened
  from there, a request starts at the agent's first comment.
- **Comparisons and the working tree stay in the menu.** One you open with ⌥⌘C, or the working tree, becomes a
  row under Branches as a branch does, kept until you close it (× or ⌘W); Swap and Direct keep the same row. An
  agent can open its changes there too, as a comparison or the working tree, with a note offering Show.
- **What you change on a merge request shows at once.** A comment, a reply, an edit, a deletion, a resolve, a
  pending comment, Send this now, an approval or a submitted review used to read the whole request again, its
  discussions, pipeline and Overview going back to loading and the page jumping. It now shows where you made it,
  faded and saying what it waits for, and becomes the real one when GitLab or GitHub answers; nothing else
  reloads. If the host refuses, it is taken back and says why, your text back in its field. The Inbox's reply,
  resolve and Mark done do the same.
- **An agent can explain its changes.** Its notes sit under the lines they are about, in the agent's colour and
  apart from your comments, with its summary over the diff, wherever you open those changes: the working tree,
  its branch, a comparison or a merge request of that branch. Answer on a note writes a comment for the agent on
  that line; dismiss one, or all of them. They stay on this Mac and never go to GitLab or GitHub.
- **An agent can draft a merge request.** It writes the title and description from what it did, with reviewers
  and labels if it knows them, and a note offers Open: the New merge request card for its branch, filled in, for
  you to check, edit and create. Nothing is created until you do, and GitLance never pushes; a branch only on
  your Mac says to push it first, as before.
- **Connect GitLab with your SSH key.** Merge requests no longer wait for a token pasted from GitLab's site. A
  token git already keeps for the host, in the remote's URL or its credential helper, is used for the session
  when GitLab takes it, and never copied. Otherwise, when your SSH key reaches GitLab, Merge requests offer
  Connect: a token for this computer made with the key, for 30 days and renewed before it ends (or Read only,
  or Not now); Settings → Merge requests has Use SSH key. A host key not yet known is never accepted for you,
  and nothing asks for a password.
- **Less text.** The app no longer explains what a reviewer already knows: a pending comment does not say who sees
  it, Submit review just counts them, the comment box says "Comment" or "Note for your agent", the Discussions
  panel says "Pending" once, Settings rows keep a line only where it adds something (Codex's limits are in a
  tooltip), and the connect panel and Settings → Merge requests say in a line which token they need.

## 1.1.0 — 2026-10-26

- **A newer release is announced.** GitLance asks GitHub for its releases when it starts and every 6 hours, and
  when one is out, a card says what is new in it and in every release in between, with its commits. Download
  opens the release page; Skip this version keeps quiet until the next one. After that, a mark in the title bar
  opens the card again. Settings → General turns it off; GitLance → Check for Updates… asks right away. Nothing
  is downloaded or installed by itself.
- Every release now says what is new in it, followed by its commits by kind.
- **⌥↓ goes on down the list.** The next merge request is the first one below the open one that is not reviewed
  and not yours, counted from where the open one was — not from Reviewed, where reviewing it to the end moved it;
  with none below, it starts again at the top, once round.
- A request's Overview lists its reviewers beside the description, and nothing else: the discussions are in the
  Discussions window on the right.

## 1.0.1 — 2026-10-24

- **The background fetch is on by default.** Every repository with a remote is fetched in the background, so one
  opened for the first time sees other people's pushes and force pushes as versions. Settings → Repository turns
  it off for one; repositories turned on before stay on. A plain folder or a repository without a remote does not
  fetch.

## 1.0.0 — 2026-10-24

GitLance 1.0 is a new window, built around what a review is made of.

- **Four spaces** in the menu on the left — Inbox, Code, Branches and Merge requests (⌃⌘1…4). Each keeps the page
  it was left on, with a bar over it; esc goes one page up, ⌘[ ⌘] go back and forward across spaces.
- **Inbox**: mentions, review requests, replies and discussions resolved by someone else, and what your agents
  answered — each answerable in place.
- **Code**: the project's tree at the working tree, `HEAD`, a branch or a commit; files and folders open as tabs;
  a note for your agent on a line, a file or a folder. A folder that is not a git repository opens too.
- **Merge requests** grouped by what each needs from you; a request as Overview · Commits · Changes; merged and
  closed ones too; what changed since the version you viewed. A branch off the main line is a page of its own.
- **GitHub pull requests**, at parity with GitLab merge requests.
- **Creating a merge request** in place of the diff: the target's template, reviewers from CODEOWNERS, labels,
  milestone — or the quick way from ⌘K.
- **Reviewing**: every file of a diff in one scroll, reviewed marks that move on, zen with one file at a time at a
  reading width, regular expressions in find, a changed Markdown file read as a page.
- **Discussions**: mentions with people's names and pictures, reactions, editing and deleting your own notes,
  completion of people and emoji, an outdated comment followed line by line across versions.
- **Agents**: several chats at once, each in its own worktree; a note goes to the chat on its branch; the agent can
  ask back with a form; a file another chat is changing says so. Codex chats are listened to too (limited).
- **Look and feel**: five themes and your own, with an editor that checks contrast; tree-sitter highlighting for
  the main languages; IntelliJ's file icons; motion; narrow windows that fold; key presets — GitLance, Android
  Studio, VS Code.
- A welcome window that connects GitLab or GitHub and clones from a URL; a project's own name and colour.
- **Report a problem** from the app (⌥⌘/).

## 0.2.0 — 2026-10-14

- **Comments for your agent go to its chat and come back answered.** GitLance is an MCP server for Claude Code and
  Codex: send the purple comments to a chat, the agent changes the code and answers each one, and you resolve or
  reopen them in GitLance. Settings → Agents connects Claude Code.
- On Windows and Linux the shortcuts use Ctrl, not the Windows key.

## 0.1.0 — 2026-10-14

The first release.

- Browse a repository's branches and commits, the working tree against `HEAD`, and compare any two revisions.
- **Versions of a rewritten branch**: its earlier versions from the reflogs, any two of them compared — a plain
  diff on the same base, the two patches on a moved one.
- Diff options: split or unified; lines, words or structural (difftastic); wrap, all lines, hide whitespace;
  changed images shown as images.
- Review aids: reviewed marks per file, find in the diff and in all of its files, who wrote each line, the
  functions a file changes, bookmarks, recent places, generated files folded, zen, the keyboard card.
- **GitLab merge requests**: discussions on the lines, replies, resolving, pending comments, approving and
  submitting a review, the pipeline, versions, creating one; GitLab links open in the app; a bell for mentions,
  review requests and replies.
- Comments for an agent, copied as markdown.
- Repository tabs, a command palette, rebindable keys, ⌘+ ⌘− ⌘0 for the whole interface.
- Builds for macOS (universal), Windows and Linux.
