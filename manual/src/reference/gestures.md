# Mouse gestures

This page lists what each mouse gesture does, place by place. It covers the mouse and the touchpad only. Keys are listed on the [Shortcuts](shortcuts.md) page.

On macOS, read <kbd>Cmd</kbd> wherever the table says <kbd>Ctrl</kbd>.

Three rules hold everywhere:

- **You act on the object itself.** You click or drag the curve, the cell or the handle, not a mode.
- **The same gesture does the same thing in every place.** <kbd>Ctrl</kbd>+wheel zooms, a double-click opens the settings or resets the view, and a right-click opens the menu of what is under the pointer.
- **Drawing tools are chosen in the Tools toolbar** (**Pointer**, **Zoom**, **Text**, **Rectangle**, **Ellipse**, **Line**, **Arrow**). A plain drag in the graph depends on the active tool.

## Graph area

A plain drag on an empty part of the plot does nothing with the **Pointer** tool. To move the view, hold <kbd>Ctrl</kbd> or use the middle button.

| Gesture | Where | What it does |
|---|---|---|
| Click | A curve, a free text, a shape, a title or the legend | Selects it. Only the **Pointer** tool selects a curve. |
| Click | An empty part of the plot | Deselects. Makes the clicked layer the active layer. With the **Text** tool, places a text. |
| Drag | A free text, a shape, a title or the legend, with the **Pointer** tool | Moves it. The width handles of a text resize it. |
| Drag | The plot, with the **Zoom** tool | Draws a zoom rectangle and zooms to it. |
| Drag | The plot, with a shape tool | Draws the shape. A click without moving places a shape of the default size. |
| <kbd>Ctrl</kbd>+drag, or middle button+drag | The plot, with any tool | Moves the view (pan). It works even over a text, a shape or the legend, and nothing is moved or drawn. |
| <kbd>Ctrl</kbd>+wheel, or pinch on a touchpad | The plot | Zooms in or out around the pointer. |
| Wheel alone | The plot | Nothing. |
| Double-click | A free text, a title, a legend entry | Edits it in place. |
| Double-click | A curve, a shape | Opens its settings. |
| Double-click | An empty part of the plot, the margins | Resets the zoom. |
| Right-click | An object, or the plot | Selects what is under the pointer and opens its menu. |

The pan is decided when you press the button: if you release <kbd>Ctrl</kbd> during the drag, the pan continues; if you press it during the drag, nothing changes.

## Axes

An axis is the strip of ticks and numbers just outside the plot area, together with its line (the edge of the plot area). The gestures below work on both. On the line, the axis takes the gesture even if a curve runs along the edge.

| Gesture | Where | What it does |
|---|---|---|
| <kbd>Ctrl</kbd>+wheel | An axis | Zooms this scale only. |
| <kbd>Ctrl</kbd>+drag, or middle button+drag | An axis | Moves this scale only. |
| Double-click | An axis | Opens the settings of the axis. |
| Drag (plain) | An axis | Nothing. |
| Right-click | An axis | Opens the menu of the axis. |

## Legend and titles

| Gesture | Where | What it does |
|---|---|---|
| Click | The legend or a title | Selects it. |
| Drag, with the **Pointer** tool | The legend or a title | Moves it. **Reset Position** in its menu puts it back. |
| Double-click | A title, an axis title | Edits the text in place. |
| Double-click | A legend entry | Opens the editing of that entry. |
| Right-click | The legend, an entry or a title | Opens its menu. |

## Analysis preview

The preview in an analysis window follows the graph, with a few differences. For peak detection, a selector with two choices, **Point** and **Zone**, sets what a plain click or drag does. The other analyses have only one gesture.

| Gesture | Where | What it does |
|---|---|---|
| Click | The preview, **Point** gesture | Adds a point, a peak or a bound, depending on the analysis. Hold <kbd>Shift</kbd> to snap to the curve. |
| Drag | The preview, **Zone** gesture | Draws a zone or a range. |
| <kbd>Shift</kbd>+wheel | The preview | Changes the number of points averaged by the snap, one point per notch. The view does not move. |
| <kbd>Ctrl</kbd>+wheel, or pinch | The preview | Zooms around the pointer. |
| <kbd>Ctrl</kbd>+wheel | An axis of the preview | Zooms this scale only. |
| <kbd>Ctrl</kbd>+drag, or middle button+drag | The preview, or one of its axes | Moves the view, or this scale only. |
| Double-click | The preview | Resets the view to the original frame. |
| Right-click | The preview | Removes the nearest point or zone. |
| Drag | A handle of the calculation, drawn on the graph | Moves it. The preview updates at once. |

## Sheet grid

The wheel scrolls the sheet. <kbd>Ctrl</kbd>+wheel does nothing special here.

| Gesture | Where | What it does |
|---|---|---|
| Click | A cell | Selects the cell. |
| <kbd>Ctrl</kbd>+click | A cell | Adds the cell to the selection, or removes it. |
| <kbd>Shift</kbd>+click | A cell | Extends the last rectangle to this cell. |
| Drag | Cells | Selects a rectangle. |
| <kbd>Ctrl</kbd>+drag | Cells | Adds a rectangle. The earlier ones stay selected. |
| Double-click | A cell or a header field | Edits it in place. |
| Right-click | A cell, a header field or a row number | If it is outside the selection, selects it first. Opens the menu for the selection. |

## Column headers and row numbers

| Gesture | Where | What it does |
|---|---|---|
| Click | A column letter | Selects the column. |
| <kbd>Ctrl</kbd>+click | A column letter | Adds the column to the selection, or removes it. |
| <kbd>Shift</kbd>+click | A column letter | Extends the selection to this column. |
| <kbd>Ctrl</kbd>+drag | Column letters | Adds a range of columns. |
| Drag | Column letters | Selects the columns you cross. |
| Drag | The left or right edge of the selected columns, under the row of letters | Drops them on a graph to plot them. |
| Drag | The top or bottom edge of selected rows, when the sheet shows the x' row | Drops them on a graph to plot the rows. |
| Click | A row number | Selects the row. |
| <kbd>Shift</kbd>+click, drag | Row numbers | Selects a range of rows. |
| Click | The corner box **#** | Selects the whole sheet. |
| Double-click | A header field | Edits it in place. |
| Right-click | A column letter | Opens the column menu: **Plot**, **Add Column**, copy and paste, **Type**, **Role**, **Move**, **Reset Width**, **Clear**, **Delete**. |

There is no drag to reorder columns. Use **Move** in the column menu instead.

## Sheet tabs and other tab strips

| Gesture | Where | What it does |
|---|---|---|
| Click | A tab | Makes it the active tab. |
| Double-click | A tab that can be renamed | Renames it in place. |
| Right-click | A tab | Opens the tab menu. It includes **Reset Column Widths** for a sheet. |
| Right-click | The empty part of the strip, or the add button | Opens the add menu. |

## Project Manager

The whole row responds to the mouse, except the visibility box.

| Gesture | Where | What it does |
|---|---|---|
| Click | A window row | Selects it and opens it. |
| Click | A folder row | Selects it. Opens it if it was closed. It does not close an open folder. |
| <kbd>Ctrl</kbd>+click | A row | Adds the row to the selection, or removes it. |
| <kbd>Shift</kbd>+click | A row | Selects the range from the anchor to this row, in tree order. A folder in the range brings its windows with it. <kbd>Ctrl</kbd>+<kbd>Shift</kbd>+click adds the range to the selection. |
| Slow click | The only selected row | Renames it in place. A quick double-click does not rename. |
| Double-click | A window row | Brings the window to the front, or restores it. It never closes it. |
| Double-click | A folder row | Closes the folder. |
| Right-click | A row | If the row is outside the selection, selects it first. Opens the menu of the object. |
| Right-click, **Add Folder** | A folder | Creates a folder in it and puts its name in editing. |
| Wheel | The list | Scrolls. |

Rows of the Project Manager cannot be dragged.

## Object Manager

| Gesture | Where | What it does |
|---|---|---|
| Click | A row | Selects the object. For a layer, also makes it the active layer. |
| Double-click | A row | Opens the settings of the object. |
| Drag | A row | Reorders the objects. On a graph, moves the object. |
| Click | The visibility box | Shows or hides the object. |
| Right-click | A row | Opens the menu of the object. |
| Right-click | The background of the panel | Opens the settings of the panel. |
| Wheel | The list | Scrolls. |

## Windows and detached windows

| Gesture | Where | What it does |
|---|---|---|
| Drag | The title bar | Moves the window. Dropping it outside the main window detaches it. |
| Drag | An edge or a corner | Resizes the window. |
| Click | An icon of the title bar (<img class="icon" src="../images/icons/caret-down.neutral.svg" alt=""> <img class="icon" src="../images/icons/arrow-square-out.neutral.svg" alt=""> <img class="icon" src="../images/icons/arrow-square-in.neutral.svg" alt=""> <img class="icon" src="../images/icons/corners-out.neutral.svg" alt=""> <img class="icon" src="../images/icons/x.neutral.svg" alt="">) | Minimizes, detaches, reattaches, maximizes or closes the window, depending on the icon. |
| Click | A window behind the others | Brings it to the front. The click also acts on what it targets. |
| Click | Outside a modal dialog | Nothing. The outline of the dialog blinks. |

A detached window is a separate window of your system. It has the same graph and sheet gestures as a floating one. Use its own title bar to move or resize it.

## Fields and pickers

| Gesture | Where | What it does |
|---|---|---|
| Wheel | A number field under the pointer | Changes the value by one step, or by one entry of the proposed list. The window does not scroll. |
| Click | The arrow of a list field | Opens the list. |
| Click, drag | The square, the wheel or the bar of the color picker | Chooses the color. |
| Right-click | A saved color | Removes it. |
| Right-click | A row number in the import preview | Opens the row menu. |
