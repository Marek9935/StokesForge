# Script Settings

A Script Settings object holds the settings of a Python script, as a TOML text, inside your project. The script reads them by name, and you change them in a form instead of editing the script. It is saved in the `.stf` file with everything else, and every change can be undone with <kbd>Ctrl</kbd>+<kbd>Z</kbd>.

## Create or import one

- **File ▸ New ▸ Add Script Settings** adds a Script Settings object named Settings1, Settings2… in the folder you selected in the Project Manager (the root if none). It starts with a small commented example that shows every kind of field. The window opens at once.
- **File ▸ Import ▸ Script Settings…** reads a `.toml` file into a new object, named after the file. You can also drop a `.toml` file on the main window.

The TOML must be exact. A file that is not valid TOML creates nothing: the status bar names the line and the reason. StokesForge does not repair or guess.

In the Project Manager, a right-click on the object offers Open and Close, the clipboard entries, Rename and Duplicate, Delete, then **Export…**. **Export…** writes a `.toml` file with exactly the text stored in the project, comments and spacing included. Double-click the object, or press <kbd>Enter</kbd> on it, to open its window.

## The Form tab

The Form tab shows one page. Each `[table]` of the file is a fixed bold heading, and each key is a row with its name on the left and a field on the right.

| Value | Field |
|---|---|
| `true` / `false` | A check box. It applies at once. |
| Text | A text field. |
| Whole number or real number | A number field. A comma is read as the decimal point. Something that is not a number is refused, in red, and the old value comes back. |
| Date | A text field that expects a date such as `2026-11-16`. |
| List | One small field per value, a red bin to remove it and an **Add Value** button to add a copy of the last value. An empty list has no value to copy: **Add Value** is greyed and its tooltip says to add the first value in the Text tab. |

A field applies when you press <kbd>Enter</kbd> or <kbd>Tab</kbd>, or click elsewhere. <kbd>Esc</kbd> cancels. Each applied change is one entry in the history, and nothing else in the text moves: comments, order and spacing stay as you wrote them.

You can only change values in the form. To add or remove a key or a section, use the Text tab.

### Hints in comments

The comment lines directly above a key, with no blank line between, are shown under the field as its help. A line that has one of these forms is a hint, and shapes the field:

- `choices: gauss | lorentz | voigt` makes a drop-down list;
- `between 1 and 1000` bounds a number, both ends included;
- `at least 0` and `at most 10` set one bound.

A line that does not read exactly like this is plain help. A number typed outside its bounds is brought back to the bound and the field shows an orange frame for two seconds. A value already in the text that is outside its hint, because a script wrote it or you typed it in the Text tab, shows in orange with a tooltip, and is never rewritten.

## The Text tab

The Text tab is the raw text, in an editor. It applies when you leave the editor, as one history entry. <kbd>Esc</kbd> abandons what you typed.

If the text is not valid TOML, it is kept and saved anyway. The editor shows the line and the reason in red under the text, and the Form tab is frozen: it shows the last readable form greyed out, under a red banner that names the line. Fix the text and the form comes back.

An empty text shows "No settings yet. Add a key in the Text tab".

## In a script

A script reads the settings with `pyst.settings` and writes them with `pyst.set_settings`. Both take the handle of the object, which you get like any other object with `pyst.resolve`.

```python
cfg = pyst.settings(pyst.resolve("[Settings1]"))
print(cfg["title"])            # Raman, sample A
print(cfg["fit"]["model"])     # gauss
for peak in cfg["peak"]:       # [[peak]] gives a list of dictionaries
    print(peak["name"], peak["centre"])

pyst.set_settings(pyst.resolve("[Settings1]"), {
    "title": "Raman, sample B",
    "fit": {"iterations": 500},   # a table is a nested dictionary
})
```

`pyst.settings` returns the keys in the order of the file. Text gives `str`, whole numbers give `int` (exact, even beyond 2^53), real numbers give `float`, `true` and `false` give `bool`. A date gives `datetime.date`, a date with a time gives `datetime.datetime`, and a time alone gives `datetime.time`. A list gives a `list`, a `[table]` a nested dictionary, and `[[table]]` a list of dictionaries.

`pyst.set_settings` takes a dictionary and changes only the keys you name:

- a key that does not exist is **created**; the keys you do not name stay as they are;
- a key that exists must keep its type: writing text into a number raises `InvalidValueError` that names the key (for example `Settings1: fit.iterations: expected integer, got string`), and **nothing is written**, not even the other keys of the same call. A whole number is accepted for a real-number key;
- comments, order and spacing stay as you wrote them;
- the hints in comments (`choices:`, `between`, `at least`, `at most`) do not refuse a script. A value written outside its hint shows in orange in the form;
- a `[[table]]` list must have as many entries as the file has; adding or removing blocks is done in the Text tab;
- a call that changes nothing writes nothing.

One run of the script is one undo entry, so <kbd>Ctrl</kbd>+<kbd>Z</kbd> undoes all the settings and everything else the run changed.

If the stored text is not valid TOML (you kept it as typed in the Text tab), `pyst.settings` and `pyst.set_settings` raise `InvalidValueError` with the name of the object, the line and the reason, for example `Settings1: line 3, column 5: …`. Fix the text and run again.

A handle that is not a Script Settings object raises `InvalidValueError`. A script cannot create a Script Settings object yet: create it with **File ▸ New ▸ Add Script Settings** (or import a `.toml` file), then let the script read and write it.

