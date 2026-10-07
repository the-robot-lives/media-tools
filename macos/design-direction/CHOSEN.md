# Chosen Direction — Darkroom (directional, not prescriptive)

Decided 2026-09-16, Gate B. Reference image: `mw-direction-darkroom.png`.

## The decision

**Darkroom governs the judging surfaces. Everything else stays conventional.**

Six of the app's seven phases — authoring, provider selection, config, file
browsing — are commodity interactions a hundred apps have solved. Inventing
visual identity there is cost without return. The one moment that belongs to
*this* tool is looking at several candidates and deciding which survives.
Darkroom is the only direction organised around that moment, and it arrived at
a workable grading UI (scored thumbnail row) without being asked.

So the direction is a **weighting, not a skin**:

| surface | treatment |
|---|---|
| Results, Grade, candidate compare | Darkroom — artifact dominant, chrome recedes, dark field |
| Library, Runs | conventional macOS list/source-list |
| Settings, Keys, Preferences | conventional — tobor-kit form on a normal sheet |
| Editor, Compose review | conventional, readable; document-like, not austere |

An app that is austere in its settings panel is just annoying.

## Divergence licence — READ THIS BEFORE BUILDING

**The mockup is a reference, not a specification.** None of the five generated
images is ideal, and the design work is explicitly permitted — expected — to
diverge from `mw-direction-darkroom.png` wherever the real design requires it.

What carries over is the *intent*: artifact dominant, chrome recedes, candidates
visible together with their scores, dark field so generated colour dominates.

What does **not** carry over is any literal detail of that render. Its labels,
control placement, proportions and toolbar are model-invented noise. Do not
reproduce them. Do not treat the near-total absence of chrome as a mandate —
the real screen must still show the prompt, the resolved provider/model, and a
way back to the run. That austerity is the reference image's main weakness and
the most likely place to diverge.

## Borrow deliberately

| from | take | for |
|---|---|---|
| Blueprint | annotated plates with explicit numeric scores | Grade screen |
| Atelier | document with revision marks in the margin | Revise screen (prompt diffs) |
| Instrument Deck | run-queue rail | sidebar / Runs |

## Rejected, with reasons

- **Atelier** and **Blueprint** scored highest (0.900) and depict a manuscript
  editor and a mechanical-CAD tool respectively. Their beauty was doing
  persuasive work their fitness did not earn. Recorded because that is a bias
  worth catching, not just a scoring quirk.
- **System Native** is the serious alternative and the right answer under
  schedule pressure — free, ages well, tobor-kit lands in it without friction.
  It loses because it makes judging candidates feel like reading a file list,
  and judging candidates is the job.
- **Instrument Deck** taught us nothing: "pro audio workstation" hijacked the
  render into an actual synthesiser before the direction could be evaluated.

## Scores are not a ranking

All five passed eval. The two highest depicted the wrong product. The criteria
(`direction_legible` / `macos_native` / `distinct`) never asked whether the
image showed a media-generation review tool. Do not use these numbers to
justify a direction — see the note in the generating commit.
