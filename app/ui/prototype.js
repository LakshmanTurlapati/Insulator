const state = {
  screen: "chat",
  rail: "changes",
  settings: "agents",
  overlay: null,
  theme: "dark",
  tab: "adjust",
  hideSide: false,
  hideRail: false,
  runOpen: false,
  termOpen: true,
  matrix: false,
  filter: "active",
};

const history = [];
let cursor = 0;

const windowEl = document.getElementById("window");
const stage = document.getElementById("stage");
const rail = document.getElementById("rail");
const toastEl = document.getElementById("toast");
let toastTimer = 0;

document.querySelectorAll(".sidebar .side-scroll .row").forEach((row) => {
  if (row.querySelector(".matrix") || row.classList.contains("view-all")) return;
  if (row.querySelector(".ico")) return;
  const mark = document.createElement("span");
  mark.className = "matrix opt";
  row.prepend(mark);
});

function snapshot() {
  return {
    screen: state.screen,
    rail: state.rail,
    settings: state.settings,
    tab: state.tab,
    hideSide: state.hideSide,
    hideRail: state.hideRail,
    runOpen: state.runOpen,
    termOpen: state.termOpen,
    filter: state.filter,
  };
}

function remember() {
  const snap = snapshot();
  const current = history[cursor];
  if (current && JSON.stringify(current) === JSON.stringify(snap)) return;
  history.splice(cursor + 1);
  history.push(snap);
  cursor = history.length - 1;
}

function restore(snap) {
  Object.assign(state, snap);
  state.overlay = null;
  render();
}

function toast(message) {
  toastEl.textContent = message;
  toastEl.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => {
    toastEl.hidden = true;
  }, 1700);
}

function layoutFor(screen) {
  if (screen === "chat" || screen === "diff") return "chat";
  return "wide";
}

function render() {
  document.documentElement.dataset.theme = state.theme;
  document.documentElement.dataset.matrix = state.matrix ? "on" : "off";
  stage.classList.toggle("is-ios", state.screen.startsWith("ios"));
  windowEl.dataset.layout = layoutFor(state.screen);
  windowEl.classList.toggle("is-settings", state.screen === "settings");
  windowEl.classList.toggle("is-side-hidden", state.hideSide && state.screen !== "settings");
  windowEl.classList.toggle("is-rail-hidden", state.hideRail);
  rail.classList.toggle("is-run-open", state.runOpen);
  rail.classList.toggle("is-term-open", state.termOpen);
  document.getElementById("run-chev").textContent = state.runOpen ? "▾" : "▸";

  document.querySelectorAll(".main > .view").forEach((view) => {
    view.classList.toggle("is-on", view.dataset.view === state.screen);
  });
  document.querySelectorAll("#phone .view").forEach((view) => {
    view.classList.toggle("is-on", state.screen === `ios-${view.dataset.ios}`);
  });
  document.querySelectorAll("[data-thread]").forEach((thread) => {
    thread.hidden = thread.dataset.thread !== state.tab;
  });
  document.querySelectorAll(".tabs .tab").forEach((tab) => {
    tab.classList.toggle("is-on", tab.dataset.tab === state.tab);
  });
  document.querySelectorAll("[data-rail-panel]").forEach((panel) => {
    panel.classList.toggle("is-on", panel.dataset.railPanel === state.rail);
  });
  document.querySelectorAll(".rail-tabs [data-rail]").forEach((tab) => {
    tab.classList.toggle("is-on", tab.dataset.rail === state.rail);
  });
  document.querySelectorAll("[data-settings-page]").forEach((page) => {
    page.classList.toggle("is-on", page.dataset.settingsPage === state.settings);
  });
  document.querySelectorAll(".set-nav .nav").forEach((nav) => {
    nav.classList.toggle("is-on", nav.dataset.settings === state.settings);
  });
  document.querySelectorAll(".overlay").forEach((overlay) => {
    overlay.classList.toggle("is-on", overlay.dataset.overlay === state.overlay);
  });
  document.querySelectorAll("[data-theme-set]").forEach((button) => {
    button.classList.toggle("is-on", button.dataset.themeSet === state.theme);
  });
  document.getElementById("matrix-toggle").classList.toggle("is-on", state.matrix);

  document.querySelectorAll(".sidebar .side-scroll .row").forEach((row) => {
    let on = false;
    if (state.screen === "home" || state.screen === "home-empty") on = row.dataset.go === "home";
    else if (state.screen === "create") on = row.dataset.go === "create";
    else if (state.screen === "history") on = row.classList.contains("view-all");
    else if ((state.screen === "chat" || state.screen === "diff") && row.dataset.tab === state.tab) on = true;
    else if (state.screen === "pr" && row.dataset.go === "pr") on = true;
    row.classList.toggle("is-selected", on);
  });

  document.querySelectorAll("#file-list .frow").forEach((row, index) => {
    row.classList.toggle("is-on", state.screen === "diff" && index === 0);
  });

  document.querySelectorAll(".jump-screen").forEach((button) => {
    let on = button.dataset.go === state.screen;
    if (button.dataset.rail && button.dataset.rail !== state.rail) on = false;
    if (button.dataset.settings && button.dataset.settings !== state.settings) on = false;
    if (button.dataset.footer === "run" && !state.runOpen) on = false;
    if (button.dataset.footer === "terminal" && !state.termOpen) on = false;
    if (button.dataset.overlayOpen) on = state.overlay === button.dataset.overlayOpen;
    button.classList.toggle("is-on", on);
  });

  document.querySelectorAll("#history-filters .chip").forEach((chip) => {
    chip.classList.toggle("is-on", chip.dataset.filter === state.filter);
  });
  document.querySelectorAll("#history-list .row").forEach((row) => {
    const archived = row.dataset.archived === "true";
    const ready = row.dataset.ready === "true" || (row.textContent || "").includes("#8376");
    const show =
      state.filter === "archived" ? archived :
      state.filter === "ready" ? ready && !archived :
      !archived;
    row.hidden = !show;
  });

  if (state.screen !== "create") document.getElementById("create-menu").classList.remove("is-on");
  if (window.Insulator?.snap) window.Insulator.paint();
}

function focusOverlay() {
  if (state.overlay === "palette") document.getElementById("palette-input").focus();
  if (state.overlay === "search") document.getElementById("file-query").focus();
  if (state.overlay === "model") document.getElementById("model-query").focus();
}

function runAction(name, el) {
  if (name === "toggle") {
    const toggle = el.classList.contains("toggle") ? el : el.closest(".toggle");
    toggle?.classList.toggle("is-on");
    const label = toggle?.closest(".field")?.querySelector("b")?.textContent || "";
    if (window.Insulator?.available()) window.Insulator.onToggle(label, toggle?.classList.contains("is-on"));
    return;
  }
  if (name === "matrix") {
    state.matrix = !state.matrix;
    render();
    return;
  }
  if (name === "toggle-side") {
    state.hideSide = !state.hideSide;
    remember();
    render();
    return;
  }
  if (name === "toggle-rail") {
    state.hideRail = !state.hideRail;
    remember();
    render();
    return;
  }
  if (name === "back") {
    if (cursor > 0) restore(history[--cursor]);
    return;
  }
  if (name === "forward") {
    if (cursor < history.length - 1) restore(history[++cursor]);
    return;
  }
  if (name === "toggle-run") {
    state.runOpen = !state.runOpen;
    render();
    return;
  }
  if (name === "toggle-term") {
    state.termOpen = !state.termOpen;
    render();
    if (state.termOpen && window.Insulator?.available()) window.Insulator.openTerminal();
    return;
  }
  if (name === "start-dev") {
    if (window.Insulator?.available()) {
      window.Insulator.startDev();
      return;
    }
    state.termOpen = true;
    state.runOpen = true;
    document.querySelector("#term-body .hint").textContent = "Dev server running on :5173";
    toast("Start Dev");
    render();
    return;
  }
  if (name === "focus-draft") {
    document.getElementById("draft").focus();
    return;
  }
  if (name === "jump") {
    const transcript = document.getElementById("transcript");
    transcript.scrollTop = transcript.scrollHeight;
    return;
  }
  if (name === "toggle-tools") {
    const detail = document.querySelector(".tool-detail");
    detail.hidden = !detail.hidden;
    return;
  }
  if (name === "send") {
    if (window.Insulator?.available()) {
      window.Insulator.sendDraft();
      return;
    }
    const draft = document.getElementById("draft");
    const text = draft.value.trim();
    if (!text) return;
    const thread = document.querySelector(`[data-thread="${state.tab}"]`);
    const bubble = document.createElement("div");
    bubble.className = "bubble";
    bubble.textContent = text;
    thread.appendChild(bubble);
    draft.value = "";
    document.getElementById("queued").hidden = true;
    toast("Message sent");
    return;
  }
  if (name === "commit") {
    if (window.Insulator?.available()) window.Insulator.commit();
    else toast("Commit is not wired");
    return;
  }
  if (name === "copy-link") {
    if (window.Insulator?.available()) window.Insulator.copyLink();
    else toast("Workspace link copied");
    return;
  }
  if (name === "attach") {
    if (window.Insulator?.available()) window.Insulator.attach();
    else toast("Attachment added");
    return;
  }
  if (name === "fast") {
    el.classList.toggle("is-on");
    if (window.Insulator?.available()) window.Insulator.chooseHarness(window.Insulator.snap?.equipped_harness || "claude", el.classList.contains("is-on"));
    return;
  }
  if (name === "new-tab") {
    if (window.Insulator?.available()) {
      window.Insulator.newChat();
      return;
    }
    state.tab = "write";
    state.screen = "chat";
    remember();
    render();
    return;
  }
  if (name === "unified") {
    document.getElementById("diff-view").classList.remove("is-split");
    el.parentElement.querySelectorAll("button").forEach((button) => button.classList.toggle("is-on", button === el));
    return;
  }
  if (name === "split") {
    document.getElementById("diff-view").classList.add("is-split");
    el.parentElement.querySelectorAll("button").forEach((button) => button.classList.toggle("is-on", button === el));
    return;
  }
  if (name === "send-comment") {
    if (window.Insulator?.available()) window.Insulator.comment();
    else toast("Comment sent to the agent");
    return;
  }
  if (name === "resolve") {
    if (window.Insulator?.available()) window.Insulator.resolve();
    else toast("Thread resolved");
    return;
  }
  if (name === "archive") {
    if (window.Insulator?.available()) window.Insulator.archive(el.dataset.id);
    return;
  }
  if (name === "pull") {
    if (window.Insulator?.available()) window.Insulator.pull();
    return;
  }
  if (name === "open-pr") {
    if (window.Insulator?.available()) window.Insulator.openPr();
    return;
  }
  if (name === "merge-pr") {
    if (window.Insulator?.available()) window.Insulator.mergePr();
    return;
  }
  if (name === "fix-errors") {
    if (window.Insulator?.available()) window.Insulator.fixErrors();
    return;
  }
  if (name === "restore") {
    if (window.Insulator?.available() && el.dataset.id) {
      window.Insulator.restore(el.dataset.id);
      return;
    }
    const row = el.closest("[data-archived]");
    if (row) {
      delete row.dataset.archived;
      row.querySelector(".meta").textContent = "Restored";
      el.remove();
    }
    toast("Workspace restored");
    render();
    return;
  }
  if (name === "create-menu") {
    document.getElementById("create-menu").classList.toggle("is-on");
    return;
  }
  if (name === "create-workspace") {
    if (window.Insulator?.available()) {
      window.Insulator.createWorkspace();
      return;
    }
    state.screen = "chat";
    state.tab = "adjust";
    state.rail = "changes";
    remember();
    toast("Workspace created");
    render();
    return;
  }
  if (name === "toast") {
    toast(el.dataset.toast || "Done");
  }
}

document.body.addEventListener("click", (event) => {
  const menu = document.getElementById("create-menu");
  if (!event.target.closest("#create-menu") && !event.target.closest("#create-from")) {
    menu.classList.remove("is-on");
  }

  if (event.target.closest("[data-overlay-close]")) {
    state.overlay = null;
    render();
    return;
  }

  const move = event.target.closest("[data-move]");
  if (move) {
    const item = move.closest(".q-item");
    const parent = item.parentElement;
    if (move.dataset.move === "up" && item.previousElementSibling) {
      parent.insertBefore(item, item.previousElementSibling);
    }
    if (move.dataset.move === "down" && item.nextElementSibling) {
      parent.insertBefore(item.nextElementSibling, item);
    }
    return;
  }

  const source = event.target.closest("[data-source]");
  if (source) {
    document.getElementById("create-source").textContent = source.dataset.source;
    menu.classList.remove("is-on");
    source.parentElement.querySelectorAll("button").forEach((button) => {
      button.classList.toggle("is-on", button === source);
    });
    return;
  }

  const flag = event.target.closest("[data-search-flag]");
  if (flag) {
    flag.classList.toggle("is-on");
    return;
  }

  const theme = event.target.closest("[data-theme-set]");
  if (theme) {
    state.theme = theme.dataset.themeSet;
    render();
    return;
  }

  const org = event.target.closest("[data-org]");
  if (org) {
    document.getElementById("org-name").textContent = org.dataset.org;
    state.overlay = null;
    toast(org.dataset.org);
    render();
    return;
  }

  const model = event.target.closest("[data-model]");
  if (model) {
    document.querySelector(".model-btn strong").textContent = model.textContent.trim();
    if (window.Insulator?.available()) window.Insulator.chooseHarness(model.dataset.model, window.Insulator.snap?.fast);
    document.querySelectorAll(".model-item").forEach((item) => {
      item.classList.toggle("is-on", item === model);
    });
    state.overlay = null;
    render();
    return;
  }

  const filter = event.target.closest("[data-filter]");
  if (filter) {
    state.filter = filter.dataset.filter;
    if (state.screen !== "history") {
      state.screen = "history";
      remember();
    }
    render();
    return;
  }

  const tabEl = event.target.closest(".tabs .tab");
  if (tabEl) {
    state.tab = tabEl.dataset.tab;
    state.screen = "chat";
    state.overlay = null;
    remember();
    if (tabEl.dataset.chatId && window.Insulator?.available()) {
      window.Insulator.invoke("select_chat", { id: tabEl.dataset.chatId }).then(() => window.Insulator.refresh());
    }
    render();
    return;
  }

  const railEl = event.target.closest(".rail-tabs [data-rail]");
  if (railEl) {
    state.rail = railEl.dataset.rail;
    if (state.screen !== "chat" && state.screen !== "diff") state.screen = "chat";
    state.overlay = null;
    remember();
    render();
    if (state.rail === "changes" && window.Insulator?.available()) window.Insulator.refreshDiff();
    return;
  }

  const scopeBtn = event.target.closest("[data-script-scope]");
  if (scopeBtn) {
    const scope = scopeBtn.dataset.scriptScope;
    document.querySelectorAll("[data-script-scope]").forEach((button) => {
      button.classList.toggle("is-on", button === scopeBtn);
    });
    document.querySelectorAll("[data-script-panel]").forEach((panel) => {
      panel.hidden = panel.dataset.scriptPanel !== scope;
    });
    return;
  }

  const settingsEl = event.target.closest(".set-nav .nav");
  if (settingsEl) {
    state.settings = settingsEl.dataset.settings;
    state.screen = "settings";
    state.overlay = null;
    remember();
    render();
    return;
  }

  const actionEl = event.target.closest("[data-action]");
  const goEl = event.target.closest("[data-go]");
  const overlayEl = event.target.closest("[data-overlay-open]");

  if (actionEl && goEl && goEl.contains(actionEl) && actionEl !== goEl) {
    runAction(actionEl.dataset.action, actionEl);
    return;
  }

  if (actionEl && !goEl) {
    runAction(actionEl.dataset.action, actionEl);
    return;
  }

  if (overlayEl && !goEl) {
    const name = overlayEl.dataset.overlayOpen;
    state.overlay = state.overlay === name ? null : name;
    render();
    focusOverlay();
    return;
  }

  if (!goEl) return;
  state.screen = goEl.dataset.go;
  if (goEl.dataset.rail) state.rail = goEl.dataset.rail;
  if (goEl.dataset.settings) state.settings = goEl.dataset.settings;
  if (goEl.dataset.tab) state.tab = goEl.dataset.tab;
  if (goEl.dataset.footer === "run") {
    state.screen = "chat";
    state.runOpen = true;
  }
  if (goEl.dataset.footer === "terminal") {
    state.screen = "chat";
    state.termOpen = true;
  }
  if (goEl.dataset.workspaceId && window.Insulator?.available()) {
    window.Insulator.openWorkspace(goEl.dataset.workspaceId);
    return;
  }
  if (goEl.dataset.chatId && window.Insulator?.available()) {
    window.Insulator.invoke("select_chat", { id: goEl.dataset.chatId }).then(() => window.Insulator.refresh());
  }
  if (goEl.dataset.file && window.Insulator?.available()) {
    window.Insulator.showFile(goEl.dataset.file);
  }
  if (goEl.dataset.go === "pr" && window.Insulator?.available()) window.Insulator.paintPullRequest();
  state.overlay = null;
  remember();
  render();
});

document.addEventListener("keydown", (event) => {
  const meta = event.metaKey || event.ctrlKey;
  const typing = document.activeElement && ["INPUT", "TEXTAREA"].includes(document.activeElement.tagName);

  if (event.key === "Escape") {
    if (state.overlay) {
      state.overlay = null;
      render();
    }
    document.getElementById("create-menu").classList.remove("is-on");
    return;
  }

  if (!meta && event.key === "?" && !typing) {
    state.overlay = state.overlay === "index" ? null : "index";
    render();
    return;
  }

  if (!meta) return;
  const key = event.key.toLowerCase();

  if (key === "k") {
    event.preventDefault();
    state.overlay = "palette";
    render();
    focusOverlay();
  } else if (key === "f" && event.shiftKey) {
    event.preventDefault();
    state.overlay = "search";
    render();
    focusOverlay();
  } else if (key === "/") {
    event.preventDefault();
    state.overlay = "shortcuts";
    render();
  } else if (key === ",") {
    event.preventDefault();
    state.screen = "settings";
    state.overlay = null;
    remember();
    render();
  } else if (key === "n" && !event.shiftKey) {
    event.preventDefault();
    state.screen = "create";
    state.overlay = null;
    remember();
    render();
  } else if (key === "b" && event.altKey) {
    event.preventDefault();
    state.hideRail = !state.hideRail;
    render();
  } else if (key === "b") {
    event.preventDefault();
    state.hideSide = !state.hideSide;
    render();
  } else if (key === "d" && event.shiftKey) {
    event.preventDefault();
    state.screen = "diff";
    state.rail = "changes";
    state.overlay = null;
    remember();
    render();
  } else if (key === "r" && event.shiftKey) {
    event.preventDefault();
    state.screen = "chat";
    state.rail = "review";
    state.overlay = null;
    remember();
    render();
  } else if (key === "r") {
    event.preventDefault();
    runAction("start-dev", document.querySelector("[data-action='start-dev']"));
  } else if (key === "l") {
    event.preventDefault();
    document.getElementById("draft").focus();
  }
});

document.getElementById("palette-input").addEventListener("input", (event) => {
  const query = event.target.value.toLowerCase();
  document.querySelectorAll("#palette-list .pal-item").forEach((item) => {
    item.hidden = query.length > 0 && !item.textContent.toLowerCase().includes(query);
  });
});

document.getElementById("settings-search").addEventListener("input", (event) => {
  const query = event.target.value.toLowerCase();
  document.querySelectorAll(".set-nav .nav").forEach((item) => {
    item.hidden = query.length > 0 && !item.textContent.toLowerCase().includes(query);
  });
  document.querySelectorAll(".set-nav .gl").forEach((label) => {
    let sibling = label.nextElementSibling;
    let any = false;
    while (sibling && !sibling.classList.contains("gl")) {
      if (sibling.classList.contains("nav") && !sibling.hidden) any = true;
      sibling = sibling.nextElementSibling;
    }
    label.hidden = query.length > 0 && !any;
  });
});

document.getElementById("model-query").addEventListener("input", (event) => {
  const query = event.target.value.toLowerCase();
  document.querySelectorAll(".model-item").forEach((item) => {
    item.hidden = query.length > 0 && !item.textContent.toLowerCase().includes(query);
  });
});

document.getElementById("file-query").addEventListener("input", (event) => {
  const query = event.target.value.trim();
  if (window.Insulator?.available()) {
    window.Insulator.search(query);
    return;
  }
  const matches = query ? document.getElementById("search-results").textContent.toLowerCase().split(query.toLowerCase()).length - 1 : 0;
  document.getElementById("search-count").textContent = query ? `${matches} matches` : "Type to search";
});

const iosDraft = document.getElementById("ios-draft");
iosDraft.addEventListener("input", () => {
  iosDraft.style.height = "auto";
  iosDraft.style.height = `${Math.min(iosDraft.scrollHeight, 120)}px`;
});

let dragging = null;
document.querySelectorAll(".q-item").forEach((item) => {
  item.addEventListener("dragstart", () => {
    dragging = item;
  });
  item.addEventListener("dragover", (event) => event.preventDefault());
  item.addEventListener("drop", (event) => {
    event.preventDefault();
    if (!dragging || dragging === item) return;
    item.parentElement.insertBefore(dragging, item);
    dragging = null;
  });
});

const params = new URLSearchParams(location.search);
const route = params.get("screen");
if (route) {
  const [screen, extra] = route.split(":");
  state.screen = screen;
  if (screen === "settings" && extra) state.settings = extra;
  if ((screen === "chat" || screen === "diff") && extra) state.rail = extra;
  if (extra === "run") {
    state.screen = "chat";
    state.runOpen = true;
  }
  if (extra === "terminal") {
    state.screen = "chat";
    state.termOpen = true;
  }
}
if (params.get("theme") === "dark") state.theme = "dark";
if (params.get("overlay")) state.overlay = params.get("overlay");
history.push(snapshot());
render();
if (window.Insulator?.available()) window.Insulator.boot();
