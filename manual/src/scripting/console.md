# The script console

The script console lets you drive StokesForge with Python. You write a script, run it, and the changes appear in your project like any other edit. This page shows how to use the console and how to run a script from a terminal. The functions you can call are listed in [The pyst API](api.md).

## What the console is

The console is a panel at the bottom of the main window, titled **Python Script**. It contains:

- a text box where you type or paste your script;
- the buttons **Run** and **Stop**;
- an output area under the buttons.

It is not a line-by-line prompt. You write a whole script, then you run it. The script text stays in the box after the run, so you can edit it and run it again.

The console embeds its own Python 3.12, with numpy and scipy included. You do not need to install anything, and your system Python is not used. Other packages (matplotlib, pandas…) are not included. The standard library is available.

Scripts reach your project through a module called `pyst`. It is already loaded: you do not need to write `import pyst`. numpy and scipy are not loaded for you; import them as usual:

```python
import numpy as np
```

If the title bar shows **— Not available in this build**, this copy of StokesForge was built without Python and the console is disabled. If you see the message "Python was not found", reinstall StokesForge.

## Run a script

1. Click in the text box and type your script.
2. Click **Run**, or press <kbd>Ctrl</kbd>+<kbd>Enter</kbd> (<kbd>Cmd</kbd>+<kbd>Enter</kbd> on macOS).

The status bar shows "Running script…" and the output area shows a busy indicator. The window stays responsive while the script runs.

The first run takes a little longer, because Python starts then. Later runs start at once.

When the script ends:

- the status bar shows "Script finished";
- the grid, the managers and the graphs update with what the script changed;
- the output area shows what the script printed.

A new console contains a short example that reads the first column of **Book1**. Replace it with your own script.

### Type several lines

The text box accepts any number of lines. Press <kbd>Enter</kbd> for a new line. <kbd>Ctrl</kbd>+<kbd>Enter</kbd> runs the script instead of adding a line. Indent with spaces, as in any Python file.

```python
a = pyst.resolve("[Book1]Sheet1!A")
values = pyst.column(a)
for i in range(3):
    print(i, values[i])
```

Pasting a script written elsewhere works the same way. The whole text is run as one script.

### Variables are kept between runs

The console keeps its Python variables from one run to the next, as an interactive Python session does. A variable you set in one run is available in the next:

```python
# First run
a = pyst.resolve("[Book1]Sheet1!A")
```

```python
# Second run
print(len(pyst.column(a)))
```

The variables disappear when you close StokesForge. The text in the box is not saved in the project file either: keep the scripts you want to reuse in a file of your own.

There is no input history. To repeat a script, edit the text and run it again.

## See the output

After the run, the output area shows up to three parts:

| Part | Content |
|---|---|
| **Standard output** | What the script printed with `print(...)` |
| **Error output** | What the script wrote to the error stream |
| **Traceback** | The Python error trace, if the script failed |

If the script printed nothing, the area shows "(no output)". Before the first run it shows "Not run yet. Write a script, then click Run".

The output appears when the run ends, not line by line. The output area scrolls when the text is long.

## Stop a script

Click **Stop** to interrupt a running script. The status bar shows "Interrupt requested", then "Script interrupted".

The stop takes effect when the script next calls `pyst`, or at the next Python instruction. A single long numpy calculation on a very large array finishes before the stop is seen.

## Errors

If your script raises an error that it does not catch, the run fails:

- the status bar shows "Script failed. See the console";
- the **Traceback** part of the output area shows the Python trace, with the line that failed.

A failed script leaves your project as it was before the run. Everything that run changed is undone, and your earlier undo history is kept.

If your script catches the error itself (with `try` and `except`), it continues, and its other changes are kept.

A script that is interrupted with **Stop** is rolled back in the same way.

## How scripts change the project

Scripts use the same operations as the menus. Every change a script makes is part of your project history.

- All the changes of one run form **one undo step**, named "Run Script". One <kbd>Ctrl</kbd>+<kbd>Z</kbd> (**Edit ▸ Undo Run Script**) removes everything the run created or changed. <kbd>Ctrl</kbd>+<kbd>Y</kbd> redoes it.
- A script that only reads data adds nothing to the history.
- A script can call `pyst.undo()` and `pyst.redo()` itself. See [The pyst API](api.md#undo-and-redo).

While a script runs, you cannot save the project, and **Run** is disabled. Wait for the script to end, or click **Stop**.

If a script has read a column and you edit that column by hand before the script writes it back, the script's write is refused rather than overwriting your edit. The script then fails with an error that says the column changed.

## Collapse, detach and resize the console

The title bar of the console has two buttons:

- **Collapse the panel** shrinks the console to its title bar. The button then reads **Expand the panel**. Running a script expands the console again.
- **Detach into a separate window** moves the console into its own window. The button then reads **Reattach to the main window**.

Drag the top edge of the panel to change its height.

## Run a script from a terminal

You can run a script without opening the window, with the `stokesforge-cli` program. This is useful for batch work on a server or for repeating the same processing on many files.

The Windows installer and the Linux archive do not include `stokesforge-cli` yet. You get it by building StokesForge from its source code.

```sh
stokesforge-cli run --project data.stf --out result.stf process.pyst
```

| Argument | Meaning |
|---|---|
| `process.pyst` | The script file, read as UTF-8 text. Required. The extension does not matter. |
| `--project data.stf` | A project to start from. Without it, the script starts with an empty project. |
| `--out result.stf` | Where to save the project when the script succeeds. Without it, nothing is saved. |

The script runs the same way as in the console, with `pyst` already loaded.

- What the script prints goes to the terminal, on the standard output and the error output.
- If the script fails, the full Python trace is printed and the program ends with a non-zero code. The `--out` file is not written.
- If the script succeeds and you gave `--out`, the program saves the project and prints "saved" followed by the path.
- If the script is interrupted, the program ends with the message "script interrupted" and does not save.

In a terminal run there is no current project file. In your script, call `pyst.save_as(path)` to save, or use `--out`. `pyst.save()` does not work there, because it needs an existing path.

### Example: process a project and save it

Write the script to a file, `normalise.pyst`:

```python
import numpy as np

y = pyst.resolve("[Book1]Sheet1!B")
data = pyst.column(y)
print("maximum:", np.nanmax(data))
pyst.set_column(y, data / np.nanmax(data))
```

Run it on a project:

```sh
stokesforge-cli run --project spectra.stf --out spectra-normalised.stf normalise.pyst
```

The same `stokesforge-cli` program can also import instrument files without a window. Run `stokesforge-cli --help` to see all its commands.

## Where to go next

- [The pyst API](api.md) lists every function a script can call, with examples.
