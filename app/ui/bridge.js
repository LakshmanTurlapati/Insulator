function terminalKey(event) {
  if (event.ctrlKey && event.key.toLowerCase() === "c") return "\x03";
  if (event.ctrlKey && event.key.toLowerCase() === "d") return "\x04";
  if (event.metaKey || event.ctrlKey || event.altKey) return null;
  if (event.key === "Enter") return "\r";
  if (event.key === "Backspace") return "\x7f";
  if (event.key === "Tab") return "\t";
  if (event.key === "Escape") return "\x1b";
  if (event.key === "ArrowUp") return "\x1b[A";
  if (event.key === "ArrowDown") return "\x1b[B";
  if (event.key === "ArrowRight") return "\x1b[C";
  if (event.key === "ArrowLeft") return "\x1b[D";
  if (event.key.length === 1) return event.key;
  return null;
}

window.Insulator = {
  scriptRunning: false,
  listening: false,
  patch: "",
  snap: null,
  attachments: [],
  pr: null,
  checks: "",
  booted: false,

  available() {
    return Boolean(window.__TAURI__ && window.__TAURI__.core);
  },

  invoke(command, args) {
    if (!this.available()) return Promise.resolve(null);
    return window.__TAURI__.core.invoke(command, args || {});
  },

  listen(name, callback) {
    if (!this.available()) return Promise.resolve(() => {});
    return window.__TAURI__.event.listen(name, callback);
  },

  ensureOutput() {
    if (this.listening || !this.available()) return;
    this.listening = true;
    this.listen("script-output", (event) => {
      const text = event.payload.text || "";
      appendLine(document.getElementById("run-log"), text);
      appendLine(document.getElementById("term-body"), text);
    });
    this.listen("terminal-output", (event) => {
      appendLine(document.getElementById("term-body"), event.payload || "");
    });
    this.listen("script-exit", () => {
      this.scriptRunning = false;
    });
    this.listen("agent-output", (event) => {
      this.appendAgentLine(event.payload.chat_id, event.payload.text || "");
    });
    this.listen("agent-exit", () => {
      this.refresh();
    });
  },

  async boot() {
    if (!this.available()) return;
    this.ensureOutput();
    await this.refresh();
    if (!this.snap?.workspaces?.length) state.screen = "home-empty";
    else if (state.screen === "chat" && !this.snap.active_chat) state.screen = "home";
    this.booted = true;
    render();
    setInterval(() => this.invoke("tick_routines").catch(() => {}), 60000);
  },

  async refresh() {
    if (!this.available()) return;
    try {
      this.snap = await this.invoke("app_snapshot");
      const snap = this.snap;
      if (snap.active_workspace && state.rail === "changes") await this.refreshDiff();
      if (snap.active_workspace && state.rail === "files") await this.paintFiles();
      if (snap.active_workspace && state.rail === "checks") await this.paintChecks();
      if (snap.active_workspace && state.screen === "pr") await this.paintPullRequest();
      this.paint();
    } catch (error) {
      toast(String(error));
    }
  },

  paint() {
    const snap = this.snap;
    if (!snap || !this.available()) return;
    const repoLabel = document.getElementById("repo-label");
    if (repoLabel) repoLabel.textContent = snap.repo_name || "repository";
    const org = document.getElementById("org-name");
    if (org && snap.repo_name) org.textContent = snap.repo_name;
    this.paintWorkspaces(snap);
    this.paintHome(snap);
    this.paintHistory(snap);
    this.paintChats(snap);
    this.paintSettings(snap);
    this.paintAgents(snap);
    this.paintRoutines(snap);
    this.paintLoadouts(snap);
    this.paintDashboard(snap);
    this.paintModels(snap);
    this.paintPalette(snap);
    this.paintMcp(snap);
    if (typeof state !== "undefined" && state.overlay === "context") this.paintContext();
    this.paintReview(snap);
    const account = document.querySelector("[data-settings-page='account'] .lede");
    if (account) account.textContent = "This app uses the Claude, Codex, Cursor, and OpenCode logins already on this Mac.";
    const organization = document.querySelector("[data-settings-page='org'] .lede");
    if (organization) organization.textContent = "Organization login is not part of this local app.";
    const repoTitle = document.querySelector("[data-settings-page='repo'] h1");
    if (repoTitle) repoTitle.textContent = snap.repo_name || "repository";
  },

  paintWorkspaces(snap) {
    const scroll = document.querySelector(".side-scroll");
    if (!scroll) return;
    const labels = [...scroll.querySelectorAll(".section-label")];
    const mine = labels.find((item) => item.textContent.trim() === "My workspaces");
    const team = labels.find((item) => item.textContent.trim() === "Team");
    const pinned = labels.find((item) => item.textContent.trim() === "Pinned");
    for (const section of [team, pinned]) {
      if (!section) continue;
      section.hidden = true;
      let sibling = section.nextElementSibling;
      while (sibling && !sibling.classList.contains("section-label")) {
        sibling.hidden = true;
        sibling = sibling.nextElementSibling;
      }
    }
    if (!mine) return;
    let node = mine.nextElementSibling;
    while (node && !node.classList.contains("view-all") && !node.classList.contains("section-label")) {
      const next = node.nextElementSibling;
      node.remove();
      node = next;
    }
    const anchor = scroll.querySelector(".view-all");
    for (const workspace of snap.workspaces.filter((item) => !item.archived)) {
      const row = document.createElement("div");
      row.className = "row";
      if (workspace.id === snap.active_workspace) row.classList.add("is-selected");
      const open = document.createElement("button");
      open.type = "button";
      open.className = "label";
      open.style.background = "none";
      open.style.border = "0";
      open.style.color = "inherit";
      open.style.flex = "1";
      open.style.textAlign = "left";
      open.dataset.go = "chat";
      open.dataset.workspaceId = workspace.id;
      open.textContent = workspace.name || workspace.branch;
      const archive = document.createElement("button");
      archive.type = "button";
      archive.className = "btn";
      archive.dataset.action = "archive";
      archive.dataset.id = workspace.id;
      archive.textContent = "Archive";
      row.append(open, archive);
      anchor?.before(row);
    }
  },

  paintHome(snap) {
    const home = document.querySelector("[data-view='home']");
    if (!home) return;
    const active = snap.workspaces.filter((item) => !item.archived);
    const heading = home.querySelector(".home-matrix b");
    if (heading) heading.textContent = `${active.length} active`;
    const list = home.querySelector(".list");
    if (!list) return;
    list.replaceChildren();
    for (const workspace of active.slice(0, 8)) {
      const button = document.createElement("button");
      button.className = "row";
      button.type = "button";
      button.dataset.go = "chat";
      button.dataset.workspaceId = workspace.id;
      const label = document.createElement("span");
      label.className = "label";
      label.textContent = workspace.name || workspace.branch;
      const meta = document.createElement("span");
      meta.className = "meta";
      meta.textContent = workspace.pr_url ? "PR open" : workspace.branch;
      button.append(label, meta);
      list.appendChild(button);
    }
    if (active.length === 0) {
      const empty = document.createElement("div");
      empty.className = "muted";
      empty.textContent = "No workspaces yet";
      list.appendChild(empty);
    }
  },

  paintHistory(snap) {
    const list = document.getElementById("history-list");
    if (!list) return;
    list.replaceChildren();
    for (const workspace of snap.workspaces) {
      const row = document.createElement("div");
      row.className = "row";
      row.dataset.archived = workspace.archived ? "true" : "false";
      row.dataset.ready = workspace.pr_url ? "true" : "false";
      const open = document.createElement("button");
      open.type = "button";
      open.className = "label";
      open.style.background = "none";
      open.style.border = "0";
      open.style.color = "inherit";
      open.dataset.go = "chat";
      open.dataset.workspaceId = workspace.id;
      open.textContent = workspace.name || workspace.branch;
      const meta = document.createElement("span");
      meta.className = "meta";
      meta.textContent = workspace.archived ? "Archived" : workspace.branch;
      row.append(open, meta);
      if (workspace.archived) {
        const restore = document.createElement("button");
        restore.type = "button";
        restore.className = "btn";
        restore.dataset.action = "restore";
        restore.dataset.id = workspace.id;
        restore.textContent = "Restore";
        row.appendChild(restore);
      }
      const archived = workspace.archived;
      const ready = Boolean(workspace.pr_url);
      row.hidden =
        state.filter === "archived" ? !archived :
        state.filter === "ready" ? !(ready && !archived) :
        archived;
      list.appendChild(row);
    }
  },

  paintChats(snap) {
    const chats = (snap.chats || []).filter((chat) => chat.workspace_id === snap.active_workspace);
    const plus = document.querySelector("[data-action='new-tab']");
    document.querySelectorAll(".tabs .tab").forEach((tab) => tab.remove());
    for (const chat of chats) {
      const button = document.createElement("button");
      button.className = "tab";
      button.type = "button";
      button.dataset.tab = chat.id;
      button.dataset.chatId = chat.id;
      if (chat.id === snap.active_chat) button.classList.add("is-on");
      const label = document.createElement("span");
      label.className = "label";
      label.textContent = chat.title || "New chat";
      button.appendChild(label);
      plus?.before(button);
    }
    const transcript = document.getElementById("transcript");
    if (!transcript) return;
    document.querySelectorAll("[data-thread]").forEach((thread) => {
      thread.hidden = true;
    });
    let live = document.getElementById("live-transcript");
    if (!live) {
      live = document.createElement("div");
      live.id = "live-transcript";
      transcript.appendChild(live);
    }
    const active = chats.find((chat) => chat.id === snap.active_chat);
    live.replaceChildren();
    if (!active) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = snap.active_workspace ? "Send a message to start a chat in this workspace." : "Create a workspace to start.";
      live.appendChild(empty);
      return;
    }
    state.tab = active.id;
    for (const message of active.messages) {
      if (message.role === "user") {
        const bubble = document.createElement("div");
        bubble.className = "bubble";
        bubble.textContent = message.text;
        live.appendChild(bubble);
      } else {
        const prose = document.createElement("div");
        prose.className = "prose";
        const paragraph = document.createElement("p");
        paragraph.textContent = message.text;
        prose.appendChild(paragraph);
        live.appendChild(prose);
      }
    }
    if (active.running) {
      const pending = document.createElement("p");
      pending.className = "muted";
      pending.id = "agent-live";
      pending.textContent = "Working…";
      live.appendChild(pending);
    }
  },

  appendAgentLine(chatId, text) {
    const snap = this.snap;
    if (snap && chatId !== snap.active_chat) return;
    let live = document.getElementById("agent-live");
    if (!live) {
      live = document.createElement("div");
      live.className = "prose";
      live.id = "agent-live";
      document.getElementById("live-transcript")?.appendChild(live);
    }
    if (live.textContent === "Working…") live.textContent = "";
    const line = document.createElement("div");
    line.textContent = text;
    live.appendChild(line);
  },

  paintSettings(snap) {
    const settings = snap.settings || {};
    const local = settings.local || {};
    const shared = settings.shared || {};
    const fill = (panel, source) => {
      const boxes = panel?.querySelectorAll(".script-box");
      if (!boxes || boxes.length < 3) return;
      if (document.activeElement && panel.contains(document.activeElement)) return;
      boxes[0].textContent = source.setup || "";
      boxes[1].textContent = (source.run && source.run[0] && source.run[0].command) || "";
      boxes[2].textContent = source.archive || "";
      boxes.forEach((box) => {
        box.contentEditable = "true";
      });
    };
    fill(document.querySelector("[data-script-panel='personal']"), local);
    fill(document.querySelector("[data-script-panel='shared']"), shared);
    const page = document.querySelector("[data-settings-page='repo']");
    if (!page) return;
    const boxes = [...page.querySelectorAll(".script-box")];
    const glob = boxes[boxes.length - 1];
    if (glob && document.activeElement !== glob) {
      glob.contentEditable = "true";
      if (!glob.dataset.filled) {
        glob.textContent = (settings.merged && settings.merged.file_include_globs) || "";
        glob.dataset.filled = "1";
      }
    }
    const review = page.querySelector(".review-box");
    if (review && document.activeElement !== review && !review.dataset.filled) {
      review.value = (settings.merged && settings.merged.review_prompt) || "";
      review.dataset.filled = "1";
    }
    if (!page.querySelector("#env-box")) {
      const area = document.createElement("textarea");
      area.id = "env-box";
      area.className = "input";
      area.rows = 4;
      const env = (settings.merged && settings.merged.env) || {};
      area.value = Object.entries(env).map(([key, value]) => `${key}=${value}`).join("\n");
      page.appendChild(area);
    }
    if (!page.querySelector("#save-settings")) {
      const save = document.createElement("button");
      save.id = "save-settings";
      save.type = "button";
      save.className = "primary";
      save.textContent = "Save scripts";
      save.style.marginTop = "12px";
      save.addEventListener("click", () => this.saveSettings());
      page.appendChild(save);
    }
    const gitPage = document.querySelector("[data-settings-page='git']");
    const prefs = (settings.merged && settings.merged.git) || {};
    const flags = [
      ["Automatically rename new branches", prefs.rename_branch !== false],
      ["Archive on merge", prefs.archive_on_merge !== false],
      ["Delete branch on archive", Boolean(prefs.delete_branch_on_archive)],
      ["Include link to workspace in PR description", Boolean(prefs.include_workspace_link)],
    ];
    gitPage?.querySelectorAll(".field").forEach((field) => {
      const label = field.querySelector("b")?.textContent || "";
      const match = flags.find(([name]) => name === label);
      const toggle = field.querySelector(".toggle");
      if (match && toggle && document.activeElement !== toggle) toggle.classList.toggle("is-on", match[1]);
    });
    const spotlight = [...document.querySelectorAll("[data-settings-page='experimental'] .field")].find((field) =>
      field.querySelector("b")?.textContent?.includes("Spotlight"),
    );
    spotlight?.querySelector(".toggle")?.classList.toggle("is-on", Boolean(snap.spotlight));
  },

  async saveSettings() {
    const personal = document.querySelector("[data-script-panel='personal']");
    const shared = document.querySelector("[data-script-scope='shared']")?.classList.contains("is-on");
    const panel = shared ? document.querySelector("[data-script-panel='shared']") : personal;
    const boxes = panel?.querySelectorAll(".script-box") || [];
    const page = document.querySelector("[data-settings-page='repo']");
    const globBoxes = [...(page?.querySelectorAll(".script-box") || [])];
    try {
      await this.invoke("save_project_settings_cmd", {
        shared: Boolean(shared),
        setup: boxes[0]?.textContent || "",
        run: boxes[1]?.textContent || "",
        archive: boxes[2]?.textContent || "",
        globs: globBoxes[globBoxes.length - 1]?.textContent || "",
        envText: document.getElementById("env-box")?.value || "",
        review: page?.querySelector(".review-box")?.value || "",
      });
      toast("Settings saved");
      await this.refresh();
    } catch (error) {
      toast(String(error));
    }
  },

  async onToggle(label, enabled) {
    const gitLabels = [
      "Automatically rename new branches",
      "Archive on merge",
      "Delete branch on archive",
      "Include link to workspace in PR description",
    ];
    if (label === "Spotlight testing") {
      try {
        await this.invoke("spotlight_cmd", { enabled });
        toast(enabled ? "Spotlight on" : "Spotlight off");
      } catch (error) {
        toast(String(error));
      }
      return;
    }
    if (!gitLabels.includes(label)) return;
    const gitPage = document.querySelector("[data-settings-page='git']");
    const value = (name) => {
      const field = [...gitPage.querySelectorAll(".field")].find((item) => item.querySelector("b")?.textContent === name);
      return Boolean(field?.querySelector(".toggle")?.classList.contains("is-on"));
    };
    try {
      await this.invoke("save_user_settings_cmd", {
        renameBranch: value(gitLabels[0]),
        archiveOnMerge: value(gitLabels[1]),
        deleteBranchOnArchive: value(gitLabels[2]),
        includeWorkspaceLink: value(gitLabels[3]),
        claude: "",
        codex: "",
        opencode: "",
      });
    } catch (error) {
      toast(String(error));
    }
  },

  paintAgents(snap) {
    const page = document.querySelector("[data-settings-page='agents']");
    if (!page) return;
    page.querySelectorAll(".agent-card").forEach((card) => card.remove());
    for (const harness of snap.harnesses || []) {
      const card = document.createElement("div");
      card.className = "agent-card";
      const grow = document.createElement("div");
      grow.className = "grow";
      const title = document.createElement("b");
      title.textContent = harness.label;
      const detail = document.createElement("p");
      detail.className = "muted small";
      detail.textContent = harness.installed ? harness.path : "Not installed";
      grow.append(title, detail);
      const pill = document.createElement("span");
      pill.className = "pill";
      pill.textContent = harness.installed ? "Connected" : "Missing";
      card.append(grow, pill);
      page.appendChild(card);
    }
  },

  paintRoutines(snap) {
    const view = document.querySelector("[data-view='routines'] .list");
    if (!view) return;
    view.replaceChildren();
    for (const routine of snap.routines || []) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "row";
      row.dataset.routineId = routine.id;
      const label = document.createElement("span");
      label.className = "label";
      label.textContent = routine.name;
      const meta = document.createElement("span");
      meta.className = "meta";
      meta.textContent = routine.active ? routine.schedule : "Paused";
      row.append(label, meta);
      row.addEventListener("click", () => this.invoke("run_routine", { id: routine.id }).then(() => this.refresh()));
      view.appendChild(row);
    }
    if (!document.getElementById("routine-name")) {
      const page = document.querySelector("[data-view='routine'] .page");
      if (!page) return;
      const name = document.createElement("input");
      name.id = "routine-name";
      name.className = "input";
      name.placeholder = "Routine name";
      const prompt = document.createElement("textarea");
      prompt.id = "routine-prompt";
      prompt.className = "input";
      prompt.placeholder = "What should the agent do?";
      const schedule = document.createElement("input");
      schedule.id = "routine-schedule";
      schedule.className = "input";
      schedule.placeholder = "manual, daily 09:00, or weekdays 09:00";
      const save = document.createElement("button");
      save.type = "button";
      save.className = "primary";
      save.textContent = "Save routine";
      save.addEventListener("click", async () => {
        await this.invoke("save_routine_cmd", {
          name: name.value || "Routine",
          prompt: prompt.value || "",
          schedule: schedule.value || "manual",
        });
        toast("Routine saved");
        await this.refresh();
      });
      page.prepend(save, schedule, prompt, name);
    }
  },

  paintLoadouts(snap) {
    const view = document.querySelector("[data-view='loadouts'] .starfield");
    if (!view || view.dataset.live === "1") {
      const list = document.getElementById("loadout-list");
      if (!list) return;
      list.replaceChildren();
      this.fillLoadouts(list, snap);
      return;
    }
    view.dataset.live = "1";
    view.replaceChildren();
    const list = document.createElement("div");
    list.id = "loadout-list";
    list.className = "list";
    const save = document.createElement("button");
    save.type = "button";
    save.className = "primary";
    save.textContent = "Save current harness";
    save.addEventListener("click", async () => {
      const name = this.harnessLabel(snap.equipped_harness);
      await this.invoke("save_loadout_cmd", { name });
      toast("Loadout saved");
      await this.refresh();
    });
    view.append(save, list);
    this.fillLoadouts(list, snap);
  },

  fillLoadouts(list, snap) {
    list.replaceChildren();
    for (const loadout of snap.loadouts || []) {
      const row = document.createElement("button");
      row.type = "button";
      row.className = "row";
      row.textContent = `${loadout.name} · ${loadout.harness}`;
      row.addEventListener("click", async () => {
        await this.invoke("equip_loadout", { id: loadout.id });
        toast(`Equipped ${loadout.name}`);
        await this.refresh();
      });
      list.appendChild(row);
    }
  },

  paintDashboard(snap) {
    const cards = document.querySelectorAll("[data-view='dashboard'] .card .n");
    if (cards.length < 4) return;
    const active = (snap.workspaces || []).filter((item) => !item.archived);
    const running = (snap.chats || []).filter((chat) => chat.running).length;
    const ready = active.filter((item) => item.pr_url).length;
    cards[0].textContent = String(running);
    cards[1].textContent = String(ready);
    cards[2].textContent = String(active.length);
    cards[3].textContent = String((snap.workspaces || []).filter((item) => item.archived).length);
  },

  paintModels(snap) {
    const list = document.getElementById("model-list");
    if (!list) return;
    list.replaceChildren();
    for (const harness of snap.harnesses || []) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "model-item";
      button.dataset.model = harness.id;
      if (harness.id === snap.equipped_harness) button.classList.add("is-on");
      button.textContent = harness.installed ? harness.label : `${harness.label} (missing)`;
      list.appendChild(button);
    }
    const strong = document.querySelector(".model-btn strong");
    if (strong) strong.textContent = this.harnessLabel(snap.equipped_harness);
    document.querySelector(".ghost.bolt")?.classList.toggle("is-on", Boolean(snap.fast));
  },

  paintPalette(snap) {
    const list = document.getElementById("palette-list");
    if (!list) return;
    list.querySelectorAll(".pal-item[data-tab]").forEach((item) => item.remove());
    list.querySelectorAll("[data-palette-workspace]").forEach((item) => item.remove());
    for (const workspace of (snap.workspaces || []).filter((item) => !item.archived)) {
      const button = document.createElement("button");
      button.className = "pal-item";
      button.type = "button";
      button.dataset.go = "chat";
      button.dataset.workspaceId = workspace.id;
      button.dataset.paletteWorkspace = workspace.id;
      button.textContent = workspace.name || workspace.branch;
      list.prepend(button);
    }
  },

  harnessLabel(id) {
    const found = (this.snap?.harnesses || []).find((item) => item.id === id);
    return found?.label || id || "Claude Code";
  },

  paintMcp(snap) {
    const body = document.querySelector("[data-overlay='mcp'] .body");
    if (!body) return;
    body.replaceChildren();
    const servers = snap.mcp || [];
    if (servers.length === 0) {
      const empty = document.createElement("p");
      empty.className = "muted";
      empty.textContent = "No MCP servers in ~/.claude.json. Agent CLIs still load their own config.";
      body.appendChild(empty);
      return;
    }
    for (const name of servers) {
      const row = document.createElement("div");
      row.className = "check";
      const pill = document.createElement("span");
      pill.className = "pill";
      pill.textContent = "Local";
      const copy = document.createElement("div");
      const title = document.createElement("b");
      title.textContent = name;
      copy.appendChild(title);
      row.append(pill, copy);
      body.appendChild(row);
    }
  },

  async paintContext() {
    const body = document.querySelector("[data-overlay='context'] .context-pop");
    if (!body || !this.snap?.active_workspace) return;
    let notes = [];
    try {
      notes = await this.invoke("list_notes_cmd");
    } catch {
      notes = [];
    }
    let editor = document.getElementById("context-note");
    if (!editor) {
      body.replaceChildren();
      editor = document.createElement("textarea");
      editor.id = "context-note";
      editor.className = "input";
      editor.rows = 6;
      editor.placeholder = "Workspace note";
      const save = document.createElement("button");
      save.type = "button";
      save.className = "btn";
      save.textContent = "Save note";
      save.addEventListener("click", async () => {
        await this.invoke("write_note_cmd", { name: "notes.md", text: editor.value });
        toast("Note saved");
      });
      body.append(editor, save);
    }
    if (document.activeElement !== editor && notes) {
      const text = await this.invoke("read_note_cmd", { name: "notes.md" }).catch(() => "");
      if (text) editor.value = text;
    }
  },

  paintReview(snap) {
    const panel = document.querySelector("[data-rail-panel='review']");
    if (!panel) return;
    const workspace = (snap.workspaces || []).find((item) => item.id === snap.active_workspace);
    const comments = workspace?.comments || [];
    const actions = panel.querySelector(".row-actions");
    panel.querySelectorAll(".check").forEach((node) => node.remove());
    if (comments.length === 0) {
      const empty = document.createElement("div");
      empty.className = "check";
      empty.textContent = "No review comments";
      panel.prepend(empty);
      return;
    }
    for (const comment of comments) {
      const row = document.createElement("div");
      row.className = "check";
      const title = document.createElement("b");
      title.textContent = comment.resolved ? "Resolved" : comment.path;
      const copy = document.createElement("p");
      copy.textContent = comment.text;
      row.append(title, copy);
      if (actions) panel.insertBefore(row, actions);
      else panel.appendChild(row);
    }
  },

  async paintFiles() {
    const panel = document.querySelector("[data-rail-panel='files']");
    if (!panel) return;
    const entries = await this.invoke("file_tree_cmd");
    panel.replaceChildren();
    for (const entry of entries || []) {
      const button = document.createElement("button");
      button.type = "button";
      button.className = "frow";
      button.textContent = entry.dir ? `${entry.path}/` : entry.path;
      if (!entry.dir) {
        button.dataset.go = "diff";
        button.dataset.file = entry.path;
      }
      panel.appendChild(button);
    }
  },

  async paintChecks() {
    const panel = document.querySelector("[data-rail-panel='checks']");
    if (!panel) return;
    let text = "No pull request checks";
    try {
      text = await this.invoke("checks_cmd");
    } catch (error) {
      text = String(error);
    }
    this.checks = text || "";
    let body = document.getElementById("checks-body");
    if (!body) {
      body = document.createElement("div");
      body.id = "checks-body";
      panel.prepend(body);
    }
    body.textContent = this.checks;
  },

  async paintPullRequest() {
    const view = document.querySelector("[data-view='pr']");
    if (!view) return;
    try {
      this.pr = await this.invoke("pr_status");
    } catch (error) {
      this.pr = null;
      toast(String(error));
    }
    const title = view.querySelector("h1");
    const pill = view.querySelector(".pill");
    const branch = view.querySelector(".pr-grid .mono");
    const description = view.querySelector("h2 + p, .page > p");
    const paragraphs = view.querySelectorAll(".page > p");
    if (!this.pr) {
      if (title) title.textContent = "No pull request";
      if (pill) pill.textContent = "";
      return;
    }
    if (title) title.textContent = this.pr.title || "Pull request";
    if (pill) pill.textContent = this.pr.number ? `#${this.pr.number}` : this.pr.state;
    if (branch) branch.textContent = `${this.pr.head} → ${this.pr.base}`;
    if (paragraphs[0]) paragraphs[0].textContent = this.pr.body || "";
    const activity = view.querySelector(".activity");
    if (activity) activity.replaceChildren();
  },

  sourceKind() {
    const label = document.getElementById("create-source")?.textContent || "New branch";
    if (label.includes("Existing")) return "branch";
    if (label.includes("GitHub")) return "pr";
    if (label.includes("Linear")) return "linear";
    return "new";
  },

  async createWorkspace() {
    try {
      const status = this.snap || (await this.invoke("app_snapshot"));
      if (!status || !status.repo) {
        const repo = await this.invoke("pick_repo");
        if (!repo) {
          toast("Choose a git repository");
          return;
        }
      }
      const kind = this.sourceKind();
      if (kind === "linear") {
        toast("Linear needs an account");
        return;
      }
      const prompt = document.getElementById("create-prompt").value.trim() || "workspace";
      const workspace = await this.invoke("create_workspace_cmd", { name: prompt, source: kind });
      state.screen = "chat";
      state.rail = "changes";
      remember();
      toast(workspace.path);
      await this.refresh();
      render();
    } catch (error) {
      toast(String(error));
    }
  },

  async openWorkspace(id) {
    await this.invoke("select_workspace", { id });
    state.screen = "chat";
    remember();
    await this.refresh();
    render();
  },

  async startDev() {
    this.ensureOutput();
    state.termOpen = true;
    state.runOpen = true;
    render();
    document.getElementById("run-log")?.replaceChildren();
    try {
      if (this.scriptRunning) {
        await this.invoke("run_script", { kind: "stop", scriptId: null });
        this.scriptRunning = false;
        toast("Script stopped");
        return;
      }
      await this.invoke("run_script", { kind: "run", scriptId: null });
      this.scriptRunning = true;
      toast("Run script started");
    } catch (error) {
      toast(String(error));
    }
  },

  async openTerminal() {
    this.ensureOutput();
    const body = document.getElementById("term-body");
    if (body && !body.dataset.keys) {
      body.dataset.keys = "1";
      body.tabIndex = 0;
      body.addEventListener("keydown", (event) => {
        const data = terminalKey(event);
        if (data == null) return;
        event.preventDefault();
        event.stopPropagation();
        this.invoke("terminal_write", { data });
      });
    }
    body?.focus();
    try {
      await this.invoke("terminal_start");
    } catch (error) {
      toast(String(error));
    }
  },

  async refreshDiff() {
    try {
      const report = await this.invoke("workspace_diff_cmd");
      if (!report) return;
      this.patch = report.patch || "";
      const count = document.getElementById("changes-count");
      if (count) count.textContent = String(report.files.length);
      const list = document.getElementById("file-list");
      if (!list) return;
      list.replaceChildren();
      if (!document.getElementById("checkpoint-btn")) {
        const bar = document.createElement("div");
        bar.className = "row-actions";
        bar.style.padding = "8px";
        const save = document.createElement("button");
        save.id = "checkpoint-btn";
        save.type = "button";
        save.className = "btn";
        save.textContent = "Checkpoint";
        save.addEventListener("click", () => this.checkpoint());
        const restore = document.createElement("button");
        restore.type = "button";
        restore.className = "btn";
        restore.textContent = "Restore checkpoint";
        restore.addEventListener("click", () => this.restoreCheckpoint());
        bar.append(save, restore);
        list.before(bar);
      }
      if (report.files.length === 0) {
        const empty = document.createElement("div");
        empty.className = "muted";
        empty.style.padding = "10px";
        empty.textContent = "No changes";
        list.appendChild(empty);
        return;
      }
      for (const file of report.files) {
        const button = document.createElement("button");
        button.className = "frow";
        button.type = "button";
        button.dataset.go = "diff";
        button.dataset.file = file.path;
        const name = document.createElement("span");
        name.className = "name";
        name.textContent = file.path;
        const counts = document.createElement("span");
        counts.className = "counts";
        const added = document.createElement("span");
        added.className = "count pos";
        added.textContent = `+${file.added}`;
        const removed = document.createElement("span");
        removed.className = "count neg";
        removed.textContent = `−${file.removed}`;
        counts.append(added, removed);
        button.append(name, counts);
        list.appendChild(button);
      }
    } catch (error) {
      toast(String(error));
    }
  },

  showFile(path) {
    const title = document.getElementById("diff-title");
    const body = document.getElementById("diff-body");
    if (title) title.textContent = path;
    if (!body) return;
    body.replaceChildren();
    const lines = (this.patch || "").split("\n");
    const relevant = [];
    let include = false;
    for (const line of lines) {
      if (line.startsWith("diff --git ")) include = line.includes(`b/${path}`) || line.includes(`a/${path}`);
      if (include) relevant.push(line);
    }
    const shown = relevant.length ? relevant : ["No patch for this file"];
    const before = [];
    const after = [];
    for (const line of shown) {
      const row = document.createElement("tr");
      if (line.startsWith("+") && !line.startsWith("+++")) {
        row.className = "add";
        after.push(line.slice(1));
      } else if (line.startsWith("-") && !line.startsWith("---")) {
        row.className = "del";
        before.push(line.slice(1));
      } else if (!line.startsWith("diff ") && !line.startsWith("index ") && !line.startsWith("@@") && !line.startsWith("---") && !line.startsWith("+++")) {
        before.push(line.startsWith(" ") ? line.slice(1) : line);
        after.push(line.startsWith(" ") ? line.slice(1) : line);
      }
      const gutter = document.createElement("td");
      gutter.className = "ln";
      const text = document.createElement("td");
      text.textContent = line;
      row.append(gutter, text);
      body.appendChild(row);
    }
    const beforePane = document.querySelector(".split .before");
    const afterPane = document.querySelector(".split .after");
    if (beforePane) beforePane.textContent = before.join("\n");
    if (afterPane) afterPane.textContent = after.join("\n");
  },

  async sendDraft() {
    const draft = document.getElementById("draft");
    const text = draft.value.trim();
    if (!text && this.attachments.length === 0) return;
    draft.value = "";
    try {
      await this.invoke("send_message", { text, attachments: this.attachments });
      this.attachments = [];
      await this.refresh();
      render();
    } catch (error) {
      toast(String(error));
    }
  },

  async newChat() {
    try {
      const chat = await this.invoke("new_chat_cmd");
      state.tab = chat.id;
      state.screen = "chat";
      await this.refresh();
      render();
    } catch (error) {
      toast(String(error));
    }
  },

  async chooseHarness(id, fast) {
    await this.invoke("set_harness", { harness: id, fast: Boolean(fast ?? this.snap?.fast) });
    await this.refresh();
  },

  async attach() {
    const paths = await this.invoke("pick_attachments");
    this.attachments.push(...(paths || []));
    toast(this.attachments.length ? `${this.attachments.length} attachment(s)` : "No file chosen");
  },

  async commit() {
    const title = (this.snap?.chats || []).find((chat) => chat.id === this.snap?.active_chat)?.title;
    try {
      const result = await this.invoke("commit_and_push", { message: title && title !== "New chat" ? title : "Update workspace" });
      toast(result.note);
      await this.refresh();
    } catch (error) {
      toast(String(error));
    }
  },

  async copyLink() {
    const text = await this.invoke("copy_link");
    if (text && navigator.clipboard) await navigator.clipboard.writeText(text);
    toast(text || "No workspace");
  },

  async comment() {
    let box = document.getElementById("comment-text");
    if (!box) {
      box = document.createElement("textarea");
      box.id = "comment-text";
      box.className = "input";
      box.placeholder = "Comment on this diff";
      document.querySelector(".comment")?.prepend(box);
    }
    const path = document.getElementById("diff-title")?.textContent || "diff";
    const text = box.value.trim() || box.parentElement?.innerText || "";
    if (!text.trim()) {
      toast("Write a comment");
      return;
    }
    await this.invoke("add_comment", { path, text });
    await this.invoke("send_comments");
    box.value = "";
    toast("Comment sent to the agent");
    await this.refresh();
  },

  async resolve() {
    await this.invoke("resolve_comment", { id: "" });
    toast("Thread resolved");
    await this.refresh();
  },

  async restore(id) {
    await this.invoke("restore_workspace_cmd", { id });
    toast("Workspace restored");
    await this.refresh();
    render();
  },

  async archive(id) {
    await this.invoke("archive_workspace_cmd", { id });
    toast("Workspace archived");
    await this.refresh();
    render();
  },

  async pull() {
    try {
      const text = await this.invoke("pull_cmd");
      toast(text || "Pulled latest");
    } catch (error) {
      toast(String(error));
    }
  },

  async openPr() {
    await this.paintPullRequest();
    if (this.pr?.url) {
      window.open(this.pr.url, "_blank");
      return;
    }
    toast("No pull request yet");
  },

  async mergePr() {
    try {
      await this.invoke("merge_cmd");
      toast("Merged");
      await this.refresh();
    } catch (error) {
      toast(String(error));
    }
  },

  async fixErrors() {
    await this.paintChecks();
    await this.invoke("send_message", { text: `Fix these failing checks:\n${this.checks}`, attachments: [] });
    toast("Sent checks to the agent");
    await this.refresh();
  },

  async search(query) {
    const hits = await this.invoke("search_cmd", { query });
    const results = document.getElementById("search-results");
    const count = document.getElementById("search-count");
    if (count) count.textContent = `${(hits || []).length} matches`;
    if (!results) return;
    results.replaceChildren();
    for (const hit of hits || []) {
      const block = document.createElement("div");
      block.className = "match";
      const title = document.createElement("h3");
      title.textContent = `${hit.path}:${hit.line}`;
      const pre = document.createElement("pre");
      pre.textContent = hit.text;
      block.append(title, pre);
      results.appendChild(block);
    }
  },

  async checkpoint() {
    const id = await this.invoke("checkpoint_cmd");
    this.lastCheckpoint = id;
    toast(`Checkpoint ${id}`);
  },

  async restoreCheckpoint() {
    const listed = await this.invoke("list_checkpoints_cmd");
    const id = this.lastCheckpoint || listed?.[listed.length - 1]?.id;
    if (!id) {
      toast("No checkpoint");
      return;
    }
    await this.invoke("restore_checkpoint_cmd", { id });
    toast("Checkpoint restored");
    await this.refreshDiff();
  },
};

function appendLine(node, text) {
  if (!node) return;
  const line = document.createElement("div");
  line.textContent = text;
  node.appendChild(line);
  node.scrollTop = node.scrollHeight;
}
