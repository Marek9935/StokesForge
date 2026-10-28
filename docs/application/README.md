# Application: contents

Describes `stokesforge-app` (the eframe shell) and `stokesforge-app-state` (the
state without egui). Split by family (F2, #301); the section titles are those of
the former `docs/application.md`, which the code and the skills cite as "file
§ Title". Conventions to follow: `.claude/conventions.md`, section "Application".

| I want… | Read | Main sections |
|---|---|---|
| to understand the bench principle, how the two crates share the work, the module → role table of `src/app/` | [`shell-and-code.md`](shell-and-code.md) | Two crates: the state and the shell; Code split |
| the analysis window, the Analysis menu, the Results panel | [`analysis.md`](analysis.md) | Analysis: window, menu, Results panel |
| the menu bar menus, the toolbars, the status bar | [`menus-and-toolbars.md`](menus-and-toolbars.md) | Themed menus; Toolbars; Status bar |
| floating windows, tabs, lists, geometry, detaching, panels | [`windows-and-panels.md`](windows-and-panels.md) | Confined floating windows; Overflow; Tabs, lists and selection; Window geometry and state; Maximizing and detaching; Collapsible and detached panels; Generic detaching |
| a new project, file drop, the import wizard | [`project-and-import.md`](project-and-import.md) | New project; File drag and drop; Import wizard |
| Project Manager, Object Manager, a graph pasted between projects | [`managers.md`](managers.md) | Project Manager and windows by content; Object Manager |
| the theme, the Fluent texts, the file picker, errors turned into sentences | [`theme-i18n-system.md`](theme-i18n-system.md) | Theme; Multilingual interface (Fluent); Native file picker |
| the state/shell dividing line, the script console, the bench tests, the CLI | [`state-and-console.md`](state-and-console.md) | Two modules; The script console; What Qt gave for free; Tests; `stokesforge-cli` |
| the grid: input, Tab and arrows, gutter, selection, context menus | [`grid.md`](grid.md) | The "Metadata" and x' header rows; Grid context menus; Multiple selection; Tab and arrows; Alignment |
| plotting from a workbook, graph properties, figure copy | [`graph.md`](graph.md) | Plotting from a workbook; Dragging a column selection to a graph; Graph properties |
