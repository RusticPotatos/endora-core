//! The steps of `features/owner-accepts-work-in-review.feature` (ADR 0149): the owner accepts or
//! sends back work in review on the existing "ready to read" card, and a loop's acceptance
//! policy says whether HQ closes its items itself.

use cucumber::{given, then, when};

use endora_team::application::TeamStore;
use endora_team::application::desk_ports::DeskStore;
use endora_team::application::guard_ports::GuardStore;
use endora_team::application::ports::DeskItem;
use endora_team::domain::closure::{Acceptance, Closure, DropWhy};
use endora_team::domain::{DeskId, TaskId, TaskStatus, WakeReason};

use super::Office;

impl Office {
    /// The newest "ready to read" card about the task titled `title`, decided or not.
    fn reading_card(&self, title: &str) -> DeskId {
        let task = self.task(title);
        self.hq
            .store()
            .desk()
            .expect("the desk answers")
            .into_iter()
            .filter(|d| d.kind == "ready_to_read" && d.task == Some(task))
            .map(|d| d.id)
            .max()
            .expect("a card to read")
    }

    fn waiting_cards_about(&self, title: &str) -> Vec<DeskItem> {
        let task = self.task(title);
        self.waiting_on_desk("ready_to_read")
            .into_iter()
            .filter(|d| d.task == Some(task))
            .collect()
    }

    fn owner_decides_card(&mut self, title: &str, choice: &str, words: &str) {
        let card = self.reading_card(title);
        self.refused = None;
        if let Err(why) = self.hq.decide_saying(card, choice, words) {
            self.refused = Some(why.to_string());
        }
    }
}

#[given(expr = "the owner lets the loop {string} close its work unverified")]
fn lets_close_unverified(w: &mut Office, project: String) {
    let project = w.project(&project).id;
    w.hq.set_loop_acceptance(project, Acceptance::Unverified)
        .expect("the owner chooses the policy");
}

#[then(expr = "the loop {string} accepts work as {string}")]
fn loop_accepts_as(w: &mut Office, project: String, policy: String) {
    let project = w.project(&project).id;
    assert_eq!(
        w.hq.loop_acceptance(project)
            .expect("the store answers")
            .name(),
        policy
    );
}

#[then(expr = "{string} has no closing on record")]
fn no_closing(w: &mut Office, title: String) {
    let task = w.task(&title);
    let closed = w.hq.store().closure(task).expect("the store answers");
    assert!(closed.is_none(), "{closed:?}");
}

#[then(expr = "{string} was closed by {string}")]
fn closed_by(w: &mut Office, title: String, who: String) {
    let task = w.task(&title);
    let closed = w.hq.store().closure(task).unwrap().expect("it closed");
    assert_eq!(closed.by, who, "{closed:?}");
    assert!(closed.at > 0, "when it closed is kept: {closed:?}");
}

#[then(expr = "the owner's desk holds one card to accept {string}")]
fn one_card_to_accept(w: &mut Office, title: String) {
    let cards = w.waiting_cards_about(&title);
    assert_eq!(cards.len(), 1, "{cards:?}");
}

#[then(expr = "the card to accept {string} says {string} and not {string}")]
fn the_card_says(w: &mut Office, title: String, said: String, not_said: String) {
    let cards = w.waiting_cards_about(&title);
    assert_eq!(cards.len(), 1, "{cards:?}");
    assert!(cards[0].text.contains(&said), "{}", cards[0].text);
    assert!(!cards[0].text.contains(&not_said), "{}", cards[0].text);
}

#[when(expr = "the owner accepts {string}")]
#[given(expr = "the owner accepts {string}")]
fn owner_accepts(w: &mut Office, title: String) {
    w.owner_decides_card(&title, "accept", "");
    if let Some(why) = &w.refused {
        // A refusal is the step's outcome only where a later step asks for it.
        eprintln!("accept refused: {why}");
    }
}

#[when(expr = "the owner sends {string} back")]
fn owner_sends_back(w: &mut Office, title: String) {
    w.owner_decides_card(&title, "send_back", "");
}

#[when(expr = "the owner sends {string} back, saying {string}")]
fn owner_sends_back_saying(w: &mut Office, title: String, words: String) {
    w.owner_decides_card(&title, "send_back", &words);
    assert_eq!(w.refused, None, "it was refused");
}

#[given(expr = "{string} is put back to doing behind the card's back")]
#[when(expr = "{string} is put back to doing behind the card's back")]
fn put_back_to_doing(w: &mut Office, title: String) {
    let task = w.task(&title);
    w.hq.store()
        .set_task_status(task, TaskStatus::Doing)
        .expect("the store answers");
}

#[given(expr = "the gate of {string} fails afterwards: {string}")]
#[when(expr = "the gate of {string} fails afterwards: {string}")]
fn gate_fails_afterwards(w: &mut Office, title: String, failed: String) {
    let task = w.task(&title);
    w.hq.record_gate(task, false, &failed)
        .expect("the gate's word");
}

#[when(expr = "the check passes again for {string}")]
fn check_passes_again(w: &mut Office, title: String) {
    let task = w.task(&title);
    w.hq.record_gate(task, true, "").expect("the gate's word");
}

#[then(expr = "{word} is woken by a send-back on {string}")]
fn woken_by_send_back(w: &mut Office, name: String, title: String) {
    let (member, task) = (w.member(&name), w.task(&title));
    let woken = w
        .hq
        .wakes()
        .expect("the store answers")
        .into_iter()
        .any(|k| k.task == Some(task) && k.member == member && k.reason == WakeReason::SentBack);
    assert!(woken, "{name} was not woken by a send-back");
}

#[when(expr = "{word} reads gluetun's logs again: {string}")]
fn reads_again(w: &mut Office, name: String, got: String) {
    let _ = w.reads(&name, "container_logs", "gluetun", &got);
}

#[when(expr = "the owner drops {string} as {string}")]
fn owner_drops(w: &mut Office, title: String, why: String) {
    let task: TaskId = w.task(&title);
    let closure = match why.as_str() {
        "wont_do" => Closure::Dropped(DropWhy::WontDo),
        other => panic!("not a drop reason in this feature: {other}"),
    };
    w.hq.drop_task_because(task, closure, None)
        .expect("the owner drops it");
}

#[given(expr = "the owner pauses the loop {string}")]
#[when(expr = "the owner pauses the loop {string}")]
fn owner_pauses_loop(w: &mut Office, project: String) {
    let project = w.project(&project).id;
    w.hq.change_project(project, "pause")
        .expect("the owner pauses a project");
}

#[when(expr = "the owner resumes the loop {string}")]
fn owner_resumes_loop(w: &mut Office, project: String) {
    let project = w.project(&project).id;
    w.hq.change_project(project, "resume")
        .expect("the owner resumes a project");
}

#[then(expr = "the thread of {string} holds no word from the owner")]
fn no_word_from_the_owner(w: &mut Office, title: String) {
    let thread =
        w.hq.store()
            .thread(w.task(&title))
            .expect("the store answers");
    let owner: Vec<_> = thread.iter().filter(|p| p.author == "owner").collect();
    assert!(owner.is_empty(), "{owner:?}");
}

#[then(expr = "{word} is not woken by a send-back on {string}")]
fn not_woken_by_send_back(w: &mut Office, name: String, title: String) {
    let (member, task) = (w.member(&name), w.task(&title));
    let woken = w
        .hq
        .wakes()
        .expect("the store answers")
        .into_iter()
        .any(|k| k.task == Some(task) && k.member == member && k.reason == WakeReason::SentBack);
    assert!(!woken, "{name} was woken by a send-back");
}
