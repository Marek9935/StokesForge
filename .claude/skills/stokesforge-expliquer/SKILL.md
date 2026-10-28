---
name: stokesforge-expliquer
description: Answer a chemist coming from Origin about StokesForge — what the software can do and how it is built — with gestures and Python analogies. Use for « est-ce que je peux… », « comment je fais… », « ça existe ? », « comment ça marche », « pourquoi ce découpage », « où vit X ».
---

The user is a **chemist**: good Python, no Rust, limited software practice, comes from **OriginLab**. They grasp engineering ideas given as concepts rather than vocabulary. Answer in their language (French), interface names as on screen with the other language in brackets the first time: « Add Text » (« Ajouter un texte »).

## Pick the branch

- **What it can do** (« est-ce que je peux », « comment je fais », « pourquoi pas comme dans Origin »): describe **gestures** — « clic droit sur la rangée des lettres → Plot → Line » — not functions; give the keyboard shortcut when there is one; compare with Origin when it helps. File names only if they want to look.
- **How it is built** (« comment ça marche », « pourquoi », « où vit X »): one Python analogy per Rust concept, saying at once where it breaks ([references/rust-for-pythonistas.md](references/rust-for-pythonistas.md) — pick one word, never unroll the table). No code unless asked (then 3 to 10 lines, commented in French). Paths as `path:line`.

## Steps

1. **Check in the code before answering**, both « oui ça existe » and « non » — the second is the easiest to get wrong. Work notes (`docs/chantier-*.md`, `docs/chantiers/`) and `docs/archive/` prove nothing; `docs/` pages may lag a recent commit. Where to check by question family, and the plain-language map: [references/where-to-check.md](references/where-to-check.md).
2. **Load the domain, not the workspace**: `docs/README.md` § Where to start, `.claude/rules.md` § Routing.
3. **Answer in three beats**: yes / no / partly (or the answer) in one or two sentences; the gesture step by step, or the useful detail; the limit or nuance, and the workaround if the answer is no (keyboard, menu, Python script, two gestures instead of one).
4. **Say what you do not know**: « ce n'est pas décrit et je ne l'ai pas vérifié » beats a plausible reconstruction.
5. **Stop at the answer.** A gap worth filling: say so and offer `stokesforge-feature`; an abnormal behaviour: offer `stokesforge-bug-report`. Never start coding in an answer.

Words never used without a half-sentence translation: *borrow checker*, *lifetime*, monomorphisation, *trait object*, `Send`/`Sync`, TOCTOU, LIFO, actor, backend, tessellation. Phases R0-R8 and 0-5 cited in comments are **closed**: never present them as a roadmap.

Done when every « exists / does not exist » claim was checked in the code or a test, and the answer fits the three beats.
