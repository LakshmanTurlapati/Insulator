use crate::workspace::{nid, ChatMessage, ChatRecord, Loadout, Persisted, Routine};

#[allow(dead_code)]
pub fn chats_for<'a>(state: &'a Persisted, workspace_id: &str) -> Vec<&'a ChatRecord> {
    state.chats.iter().filter(|chat| chat.workspace_id == workspace_id).collect()
}

pub fn new_chat(state: &mut Persisted, workspace_id: &str, harness: &str) -> ChatRecord {
    let chat = ChatRecord {
        id: nid("chat"),
        workspace_id: workspace_id.to_string(),
        title: "New chat".into(),
        harness: harness.to_string(),
        fast: state.fast,
        messages: Vec::new(),
        running: false,
    };
    state.active_chat = Some(chat.id.clone());
    state.chats.push(chat.clone());
    chat
}

pub fn ensure_chat(state: &mut Persisted, workspace_id: &str, harness: &str) -> String {
    if let Some(id) = state.active_chat.clone() {
        if state.chats.iter().any(|chat| chat.id == id && chat.workspace_id == workspace_id) {
            return id;
        }
    }
    new_chat(state, workspace_id, harness).id
}

pub fn append_message(state: &mut Persisted, chat_id: &str, role: &str, text: &str, attachments: Vec<String>) {
    if let Some(chat) = state.chats.iter_mut().find(|chat| chat.id == chat_id) {
        if chat.title == "New chat" && role == "user" {
            chat.title = text.chars().take(48).collect();
        }
        chat.messages.push(ChatMessage {
            role: role.into(),
            text: text.into(),
            attachments,
        });
    }
}

pub fn set_running(state: &mut Persisted, chat_id: &str, running: bool) {
    if let Some(chat) = state.chats.iter_mut().find(|chat| chat.id == chat_id) {
        chat.running = running;
    }
}

pub fn transcript(state: &Persisted, chat_id: &str) -> String {
    let Some(chat) = state.chats.iter().find(|chat| chat.id == chat_id) else {
        return String::new();
    };
    let mut text = String::new();
    for message in &chat.messages {
        text.push_str(&message.role);
        text.push_str(": ");
        text.push_str(&message.text);
        text.push('\n');
    }
    text
}

pub fn save_loadout(state: &mut Persisted, name: &str, harness: &str, fast: bool) -> Loadout {
    let loadout = Loadout {
        id: nid("loadout"),
        name: name.to_string(),
        harness: harness.to_string(),
        fast,
    };
    state.loadouts.push(loadout.clone());
    loadout
}

pub fn save_routine(state: &mut Persisted, name: &str, prompt: &str, harness: &str, schedule: &str) -> Routine {
    let routine = Routine {
        id: nid("routine"),
        name: name.to_string(),
        prompt: prompt.to_string(),
        harness: harness.to_string(),
        schedule: if schedule.trim().is_empty() { "manual".into() } else { schedule.trim().into() },
        active: true,
        last_fired: None,
    };
    state.routines.push(routine.clone());
    routine
}

pub fn due_routines(state: &Persisted) -> Vec<String> {
    let (date, time, weekday) = clock();
    state
        .routines
        .iter()
        .filter(|routine| routine.active && routine.schedule != "manual")
        .filter(|routine| routine.last_fired.as_deref() != Some(date.as_str()))
        .filter(|routine| schedule_due(&routine.schedule, &time, weekday))
        .map(|routine| routine.id.clone())
        .collect()
}

fn schedule_due(schedule: &str, time: &str, weekday: u32) -> bool {
    let mut parts = schedule.split_whitespace();
    let kind = parts.next().unwrap_or("");
    let at = parts.next().unwrap_or("00:00");
    if time < at {
        return false;
    }
    match kind {
        "daily" => true,
        "weekdays" => (1..=5).contains(&weekday),
        _ => false,
    }
}

fn clock() -> (String, String, u32) {
    let date = CommandOut::new("date", &["+%F"]);
    let time = CommandOut::new("date", &["+%H:%M"]);
    let weekday = CommandOut::new("date", &["+%u"]).parse().unwrap_or(1);
    (date, time, weekday)
}

struct CommandOut;

impl CommandOut {
    fn new(cmd: &str, args: &[&str]) -> String {
        std::process::Command::new(cmd)
            .args(args)
            .output()
            .ok()
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .unwrap_or_default()
    }
}
