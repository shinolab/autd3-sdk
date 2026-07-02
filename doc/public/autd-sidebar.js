(function () {
  var KEY = "autd-sidebar-collapsed";
  function collapsed() {
    try {
      return localStorage.getItem(KEY) === "1";
    } catch (e) {
      return false;
    }
  }
  function apply() {
    document.documentElement.classList.toggle("autd-sidebar-collapsed", collapsed());
  }
  function icon(btn) {
    btn.textContent = collapsed() ? "›" : "‹";
    btn.setAttribute("aria-pressed", collapsed() ? "true" : "false");
  }
  function ensure() {
    apply();
    var btn = document.querySelector(".autd-sidebar-toggle");
    if (!btn) {
      btn = document.createElement("button");
      btn.type = "button";
      btn.className = "autd-sidebar-toggle";
      btn.setAttribute("aria-label", "Open/close sidebar");
      btn.title = "Open/close sidebar";
      btn.addEventListener("click", function () {
        try {
          localStorage.setItem(KEY, collapsed() ? "0" : "1");
        } catch (e) { }
        apply();
        icon(btn);
      });
      document.body.appendChild(btn);
    }
    icon(btn);
  }
  if (document.readyState !== "loading") ensure();
  else document.addEventListener("DOMContentLoaded", ensure);
  document.addEventListener("astro:page-load", ensure);
})();
