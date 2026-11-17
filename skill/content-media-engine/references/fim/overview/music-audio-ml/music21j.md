# Music21j

## What
Music21j is the JavaScript port of MIT's music21 Python toolkit for computer-aided musicology, enabling music analysis and visualization in the browser. It produces notation (rendered via its bundled VexFlow) plus theory/analysis results and MIDI playback. Primary consumer is browser JavaScript.

## How
- **LLM emits:** a JavaScript ES module (`.js`/`.mjs`) exporting `buildScore(music21)`, returning a Score/Part/Stream constructed from notes, rests, chords and measures. This is JavaScript musical content, not an XML format or generated web application.
- **Content prompt:** “Generate a happy upbeat classical song in C major, 16 bars in 4/4, with a memorable melody, simple piano accompaniment and a final cadence.” Serialization belongs to the solution guide, not the user's musical brief.
- **Separate render step:** the host loads music21j and the generated module, calls `buildScore(music21)`, then `score.appendNewDOM(container)`. Playback controls and audio permissions belong to the viewer.
- **MusicXML alternative:** request the separate `musicxml` asset type for XML score interchange. music21j can parse it with `new music21.musicxml.xmlToM21.ScoreParser().scoreFromText(xmlString)`.
- **Final artifact:** the reusable musical score module; its rendered notation is a derived viewer result.

## Why
- **Reach for it when:** you want music theory/analysis capability in the browser — music education and theory apps, basic in-browser analysis, interactive notation tied to analysis, academic musicology tools, and simple notation editing. Backed by MIT's established music21 research toolkit with MusicXML import/export.
- **Limitations:** smaller feature set than the Python version; thinner documentation than the parent library; performance constraints in the browser; fewer corpus/analysis tools.
- **Relative to siblings:** Music21j is the analysis-first option — it renders notation through VexFlow and plays via Web Audio, but its differentiator is theory/analysis (key detection, intervals, scales) rather than pure engraving. Choose it when the task is understanding music, not just displaying it.

## Source
- Solution reference: `fim/solution/music21j.md`
