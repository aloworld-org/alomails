// alomails landing page — progressive enhancement only (the page is fully
// readable without it). External file so the site's CSP can forbid inline
// scripts (script-src 'self').
(function () {
  var yr = document.getElementById("yr");
  if (yr) yr.textContent = String(new Date().getFullYear());

  // Highlight the visitor's platform.
  var p = navigator.platform || "";
  var ua = navigator.userAgent || "";
  var isMac = /Mac/.test(p) || /Mac OS X/.test(ua);
  var isWin = /Win/.test(p) || /Windows/.test(ua);
  if (isMac) {
    document.getElementById("card-mac").classList.add("rec");
    document.getElementById("badge-mac").hidden = false;
  } else if (isWin) {
    document.getElementById("card-win").classList.add("rec");
    document.getElementById("badge-win").hidden = false;
  }

  // Grey out any installer not yet hosted (e.g. the macOS build arrives from
  // CI); the button re-enables automatically once the file is present.
  document.querySelectorAll(".card .btn[href]").forEach(function (a) {
    fetch(a.getAttribute("href"), { method: "HEAD" })
      .then(function (r) {
        if (!r.ok) mark(a);
      })
      .catch(function () {
        mark(a);
      });
  });
  function mark(a) {
    a.textContent = "Building — available shortly";
    a.removeAttribute("href");
    a.style.background = "#c7bfb2";
    a.style.pointerEvents = "none";
  }
})();
