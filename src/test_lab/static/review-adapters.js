/* Shared native-document hosts; artifacts never contain viewer controls. */
(function (root) {
  "use strict";
  const safeJSON = value => JSON.stringify(value).replace(/</g, "\\u003c");
  const shell = (head, body, boot) => `<!doctype html><html><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">${head}<style>body{margin:20px;background:#fff;color:#17202a;font:16px system-ui}#score{overflow:auto}#error{white-space:pre-wrap;color:#a00}</style></head><body>${body}<pre id="error" role="alert"></pre><script type="module">try{${boot};document.documentElement.dataset.review="ready";}catch(e){document.documentElement.dataset.review="error";document.getElementById("error").textContent=String(e);console.error(e);}<\/script></body></html>`;
  const osmdDocument = source => shell(
    '<script src="https://cdn.jsdelivr.net/npm/opensheetmusicdisplay@1.9.4/build/opensheetmusicdisplay.min.js"><\/script>',
    '<div id="score"></div>',
    `const score=new opensheetmusicdisplay.OpenSheetMusicDisplay("score",{autoResize:true,backend:"svg",drawingParameters:"compacttight"});await score.load(${safeJSON(source)});score.render()`
  );
  const music21jDocument = url => shell(
    '<link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/music21j@0.24.0/releases/music21j.css"><script>window.m21conf={loadSoundfont:false};<\/script><script src="https://cdn.jsdelivr.net/npm/music21j@0.24.0/releases/music21.debug.js"><\/script>',
    '<div id="score"></div>',
    `const doc=await import(${safeJSON(url)});const factory=doc.buildScore||(typeof doc.default==="function"?doc.default:null);const score=factory?await factory(music21):(doc.score||doc.default);if(!score||typeof score.appendNewDOM!=="function")throw new Error("Export buildScore(music21) returning a music21j Score or Stream.");await score.appendNewDOM(document.getElementById("score"))`
  );
  root.MediaToolReview = { osmdDocument, music21jDocument };
})(globalThis);
