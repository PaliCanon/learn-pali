//! The one-time hint toward Settings (task 059): not signed in, after the
//! first finished session, 'Use Learn on several devices? Sync with Google
//! Drive.' with 'Open Settings' and 'Not now'.
//!
//! It shows when [`visible`] says so: a sync cycle exists (the host gave a
//! transport) and is mounted, the account is `SignedOut` (remembered but
//! paused counts as signed in), a session was finished in this tab (the
//! home latches `finished`, and mounts the hint in `Mode::Home` only, so
//! never while a session runs) and it was not answered. 'Not now', 'Open
//! Settings' and any sign-in answer it ([`answer`]): the host's memory
//! (`pc_learn_hint` on the site) remembers it, and a flag in memory for the
//! tab covers a host with no memory and a write that failed.
//!
//! Hydration: the server has no transport and nothing is mounted before the
//! client's Effect, so the first paint is empty on both sides; the host's
//! memory is read only once the cycle is mounted.

use std::cell::Cell;

use leptos::prelude::*;
use leptos_i18n::td_string;
use palicanon_ui_lib::{Button, ShellState, Variant};

use crate::backend::{Hint, SyncAccount};
use crate::catalogue::strings;
use crate::sync::SyncCycle;

/// What decides whether the hint shows (see [`visible`]).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "the four independent conditions of the design's sentence, named so the truth table reads"
)]
pub(crate) struct Inputs {
    /// A sync cycle exists (the host gave a transport) and is mounted.
    pub(crate) transport: bool,
    /// The account is `SignedOut`.
    pub(crate) signed_out: bool,
    /// A session was finished in this tab.
    pub(crate) finished: bool,
    /// The hint was answered: the host's memory says so, or the tab's flag.
    pub(crate) answered: bool,
}

/// Whether the hint shows: every condition holds and it was not answered.
pub(crate) fn visible(inputs: Inputs) -> bool {
    inputs.transport && inputs.signed_out && inputs.finished && !inputs.answered
}

thread_local! {
    /// The tab's memory of an answer, for a host that gave no memory and
    /// for a write that failed. Thread-local (the page is one thread) and
    /// not a signal or a context: the home unmounts the hint when a session
    /// starts and mounts a new one when it ends, and the new one must still
    /// know. The tests reset it (`reset_tab_flag`), each test running on a
    /// thread of its own.
    static TAB_ANSWERED: Cell<bool> = const { Cell::new(false) };
}

/// Whether the tab's flag says the hint was answered.
pub(crate) fn tab_answered() -> bool {
    TAB_ANSWERED.with(Cell::get)
}

/// Whether the hint was answered: the tab's flag, or the host's memory.
fn answered() -> bool {
    tab_answered() || Hint::current().is_some_and(Hint::seen)
}

/// Whether a sign-in answers the hint now: the account is signed in
/// (remembered but paused counts) and the hint was not answered yet.
pub(crate) fn sign_in_answers(account: &SyncAccount, answered: bool) -> bool {
    !matches!(account, SyncAccount::SignedOut) && !answered
}

/// The sign-in Effect's body: answers once, through [`not_now`], when
/// [`sign_in_answers`] says so.
fn answer_on_sign_in(account: &SyncAccount, done: RwSignal<bool>) {
    if sign_in_answers(account, answered()) {
        not_now(done);
    }
}

/// Answers the hint: the host's memory remembers it, and the tab's flag is
/// set whether or not that write went through (it is cheap, and covers a
/// host with no memory and a refused write alike).
pub(crate) fn answer() {
    if let Some(hint) = Hint::current() {
        let _ = hint.remember();
    }
    TAB_ANSWERED.with(|flag| flag.set(true));
}

/// 'Open Settings': answers, takes the banner away and opens Settings
/// (`settings_open` is the shell's; none in a host without one).
pub(crate) fn open_settings(settings_open: Option<RwSignal<bool>>, done: RwSignal<bool>) {
    not_now(done);
    if let Some(settings_open) = settings_open {
        settings_open.set(true);
    }
}

/// 'Not now': answers and takes the banner away.
pub(crate) fn not_now(done: RwSignal<bool>) {
    answer();
    done.set(true);
}

/// The hint banner; the home mounts it at home only, never in a session.
#[component]
pub(crate) fn SyncHint(
    /// A session was finished in this tab (latched by the home).
    #[prop(into)]
    finished: Signal<bool>,
) -> impl IntoView {
    let sync = SyncCycle::current();
    let settings_open = use_context::<ShellState>().map(|shell| shell.settings_open);
    // The banner goes away at once when answered here.
    let done = RwSignal::new(false);
    // Any sign-in is an answer (remembered but paused counts too): once.
    Effect::new(move |_| {
        if let Some(sync) = sync {
            answer_on_sign_in(&sync.account().get(), done);
        }
    });
    move || {
        let sync = sync?;
        let transport = sync.mounted();
        visible(Inputs {
            transport,
            signed_out: matches!(sync.account().get(), SyncAccount::SignedOut),
            finished: finished.get(),
            answered: done.get() || (transport && answered()),
        })
        .then(|| banner(settings_open, done))
    }
}

/// The banner: the text, then 'Open Settings' and 'Not now'. Phone first:
/// full width, stacked; a row from `sm`.
fn banner(settings_open: Option<RwSignal<bool>>, done: RwSignal<bool>) -> impl IntoView {
    let t = strings();
    view! {
        <aside
            data-sync-hint=""
            class="flex w-full flex-col gap-3 rounded-lg border border-line p-4 sm:flex-row sm:items-center sm:justify-between"
        >
            <p class="text-sm text-heading">{move || td_string!(t.locale(), sync_hint_text)}</p>
            <div class="flex flex-wrap gap-3">
                <Button
                    variant=Variant::Accent
                    on_click=move |_| open_settings(settings_open, done)
                >
                    {move || td_string!(t.locale(), sync_hint_open)}
                </Button>
                <Button variant=Variant::Ghost on_click=move |_| not_now(done)>
                    {move || td_string!(t.locale(), sync_hint_dismiss)}
                </Button>
            </div>
        </aside>
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use palicanon_ui_lib::Language;

    use super::*;
    use crate::backend::{Backend, Hint, HintMemory, SharedSync, SyncAccount};
    use crate::store::Load;
    use crate::sync::SyncCycle;
    use crate::test_support::{FakeTransport, progress_fixture, provide_fakes};

    /// A host memory the tests drive: whether it says seen, whether its
    /// write goes through, how many writes it was asked for.
    struct FakeMemory {
        seen: Cell<bool>,
        writable: Cell<bool>,
        writes: Cell<usize>,
    }

    impl FakeMemory {
        fn new(seen: bool, writable: bool) -> Rc<Self> {
            Rc::new(FakeMemory {
                seen: Cell::new(seen),
                writable: Cell::new(writable),
                writes: Cell::new(0),
            })
        }
    }

    impl HintMemory for FakeMemory {
        fn seen(&self) -> bool {
            self.seen.get()
        }

        fn remember(&self) -> bool {
            self.writes.set(self.writes.get() + 1);
            self.writable.get()
        }
    }

    fn all() -> Inputs {
        Inputs {
            transport: true,
            signed_out: true,
            finished: true,
            answered: false,
        }
    }

    /// 'shown after the first finished session ... not signed in': all
    /// four conditions together show it.
    #[test]
    fn the_hint_shows_when_every_condition_holds() {
        assert!(visible(all()));
    }

    /// Every single missing condition hides it: no transport (nothing to
    /// sync with), signed in or remembered, no finished session ('never
    /// while a session runs', and not before the first one), answered.
    #[test]
    fn each_missing_condition_hides_the_hint() {
        let cases = [
            (
                "no transport",
                Inputs {
                    transport: false,
                    ..all()
                },
            ),
            (
                "not signed out",
                Inputs {
                    signed_out: false,
                    ..all()
                },
            ),
            (
                "no finished session",
                Inputs {
                    finished: false,
                    ..all()
                },
            ),
            (
                "answered",
                Inputs {
                    answered: true,
                    ..all()
                },
            ),
        ];
        for (name, inputs) in cases {
            assert!(!visible(inputs), "{name}");
        }
        assert!(!visible(Inputs::default()), "nothing holds");
    }

    /// What the tests set up: a transport for `account` (none: no cycle in
    /// context), mounted or not, and the host memory (none: the host gave
    /// none).
    struct Setup {
        account: Option<SyncAccount>,
        mounted: bool,
        memory: Option<Rc<FakeMemory>>,
        tab_flag: bool,
    }

    impl Setup {
        fn signed_out() -> Self {
            Setup {
                account: Some(SyncAccount::SignedOut),
                mounted: true,
                memory: None,
                tab_flag: false,
            }
        }
    }

    fn render(setup: &Setup, finished: bool, language: Language) -> String {
        Owner::new()
            .with(|| {
                reset_tab_flag();
                palicanon_ui_lib::provide_language().set(language);
                provide_fakes(Load::Ok(progress_fixture()));
                if let Some(memory) = &setup.memory {
                    provide_context(Hint::new(memory.clone()));
                }
                if let Some(account) = setup.account.clone() {
                    let transport: SharedSync = Rc::new(FakeTransport::new(account));
                    let cycle = SyncCycle::new(
                        transport,
                        Backend::current(),
                        || {},
                        String::new,
                        |_, _| {},
                    );
                    if setup.mounted {
                        cycle.set_mounted();
                    }
                    provide_context(cycle);
                }
                if setup.tab_flag {
                    TAB_ANSWERED.with(|flag| flag.set(true));
                }
                view! { <SyncHint finished=finished /> }.to_html()
            })
            .replace("<!>", "")
    }

    fn english(setup: &Setup, finished: bool) -> String {
        render(setup, finished, Language::En)
    }

    /// Tests run each on their own thread, so the tab's flag starts false;
    /// this resets it anyway for a test that reuses a thread.
    fn reset_tab_flag() {
        TAB_ANSWERED.with(|flag| flag.set(false));
    }

    const TEXT: &str = "Use Learn on several devices? Sync with Google Drive.";

    /// 'shown after the first finished session': finished and signed out
    /// shows the text and both buttons.
    #[test]
    fn a_finished_session_signed_out_shows_the_banner() {
        let html = english(&Setup::signed_out(), true);
        assert!(html.contains("data-sync-hint"), "{html}");
        assert!(html.contains(TEXT), "{html}");
        assert!(html.contains(">Open Settings<"), "{html}");
        assert!(html.contains(">Not now<"), "{html}");
    }

    /// The same banner in French and Vietnamese (one string each, from the
    /// locale files).
    #[test]
    fn the_banner_speaks_french_and_vietnamese() {
        let fr = render(&Setup::signed_out(), true, Language::Fr);
        assert!(
            fr.contains("Vous utilisez Learn sur plusieurs appareils"),
            "{fr}"
        );
        assert!(fr.contains(">Pas maintenant<"), "{fr}");
        let vi = render(&Setup::signed_out(), true, Language::Vi);
        assert!(vi.contains("Dùng Learn trên nhiều thiết bị"), "{vi}");
        assert!(vi.contains(">Mở Cài đặt<"), "{vi}");
    }

    /// 'never while a session runs' / 'after the first finished session':
    /// not finished, nothing.
    #[test]
    fn no_finished_session_renders_nothing() {
        assert_eq!(english(&Setup::signed_out(), false), "");
    }

    /// 'Remembered-but-paused counts as signed in, no hint'.
    #[test]
    fn a_remembered_account_renders_nothing() {
        let setup = Setup {
            account: Some(SyncAccount::Remembered { email: None }),
            ..Setup::signed_out()
        };
        assert_eq!(english(&setup, true), "");
    }

    #[test]
    fn a_signed_in_account_renders_nothing() {
        let setup = Setup {
            account: Some(SyncAccount::SignedIn { email: None }),
            ..Setup::signed_out()
        };
        assert_eq!(english(&setup, true), "");
    }

    /// A host with no transport (the app): no cycle in context, no hint.
    #[test]
    fn no_cycle_in_context_renders_nothing() {
        let setup = Setup {
            account: None,
            ..Setup::signed_out()
        };
        assert_eq!(english(&setup, true), "");
    }

    /// The server's paint and the first hydration pass: not mounted, so
    /// nothing (the same markup on both sides).
    #[test]
    fn a_cycle_that_is_not_mounted_renders_nothing() {
        let setup = Setup {
            mounted: false,
            ..Setup::signed_out()
        };
        assert_eq!(english(&setup, true), "");
    }

    /// 'a flag in memory for this tab': the tab's flag hides it.
    #[test]
    fn the_tab_flag_hides_the_banner() {
        let setup = Setup {
            tab_flag: true,
            ..Setup::signed_out()
        };
        assert_eq!(english(&setup, true), "");
    }

    /// '`pc_learn_hint` ... remembered': a host memory that says seen hides
    /// it.
    #[test]
    fn a_host_memory_that_says_seen_hides_the_banner() {
        let setup = Setup {
            memory: Some(FakeMemory::new(true, true)),
            ..Setup::signed_out()
        };
        assert_eq!(english(&setup, true), "");
    }

    /// A host memory that says not seen leaves the banner.
    #[test]
    fn a_host_memory_that_says_not_seen_shows_the_banner() {
        let setup = Setup {
            memory: Some(FakeMemory::new(false, true)),
            ..Setup::signed_out()
        };
        assert!(english(&setup, true).contains(TEXT));
    }

    /// 'phone: full width under the brand': the classes are phone first.
    #[test]
    fn the_banner_is_full_width_and_stacked_on_the_phone() {
        let html = english(&Setup::signed_out(), true);
        let tag = &html[html.find("<aside").expect(&html)..];
        let tag = &tag[..tag.find('>').expect(tag)];
        for class in ["w-full", "flex-col", "sm:flex-row"] {
            assert!(tag.contains(class), "{class} in {tag}");
        }
    }

    /// `answer()` writes the host memory once and sets the tab's flag.
    #[test]
    fn answering_writes_the_host_memory_once_and_sets_the_tab_flag() {
        Owner::new().with(|| {
            reset_tab_flag();
            let memory = FakeMemory::new(false, true);
            provide_context(Hint::new(memory.clone()));
            answer();
            assert_eq!(memory.writes.get(), 1);
            assert!(tab_answered());
        });
    }

    /// 'if the write fails, a flag in memory for this tab': the write is
    /// refused, the tab's flag is set all the same.
    #[test]
    fn a_refused_write_still_sets_the_tab_flag() {
        Owner::new().with(|| {
            reset_tab_flag();
            let memory = FakeMemory::new(false, false);
            provide_context(Hint::new(memory.clone()));
            answer();
            assert_eq!(memory.writes.get(), 1, "it tried");
            assert!(tab_answered(), "the tab remembers what the host could not");
        });
    }

    /// A host that gave no memory: the tab's flag alone.
    #[test]
    fn with_no_host_memory_the_tab_flag_is_all() {
        Owner::new().with(|| {
            reset_tab_flag();
            answer();
            assert!(tab_answered());
        });
    }

    /// `answered()` is the tab's flag OR the host's memory, each alone: a
    /// host whose memory says unseen is still answered once the tab's flag
    /// is set (a refused write), and one whose memory says seen is answered
    /// with a clean flag (a reload).
    #[test]
    fn answered_is_the_tab_flag_or_the_host_memory_each_alone() {
        Owner::new().with(|| {
            reset_tab_flag();
            provide_context(Hint::new(FakeMemory::new(false, false)));
            assert!(!answered(), "neither");
            TAB_ANSWERED.with(|flag| flag.set(true));
            assert!(answered(), "the tab's flag alone");
        });
        Owner::new().with(|| {
            reset_tab_flag();
            provide_context(Hint::new(FakeMemory::new(true, false)));
            assert!(answered(), "the host's memory alone");
        });
    }

    /// 'Open Settings': answers, then opens Settings.
    #[test]
    fn open_settings_answers_and_opens_settings() {
        Owner::new().with(|| {
            reset_tab_flag();
            let memory = FakeMemory::new(false, true);
            provide_context(Hint::new(memory.clone()));
            let settings_open = RwSignal::new(false);
            let done = RwSignal::new(false);
            open_settings(Some(settings_open), done);
            assert!(settings_open.get_untracked());
            assert!(done.get_untracked(), "the banner goes away");
            assert_eq!(memory.writes.get(), 1);
            assert!(tab_answered());
        });
    }

    /// A host with no shell (no Settings to open): the answer is still
    /// given, nothing else happens.
    #[test]
    fn open_settings_without_a_shell_still_answers() {
        Owner::new().with(|| {
            reset_tab_flag();
            let done = RwSignal::new(false);
            open_settings(None, done);
            assert!(tab_answered());
            assert!(done.get_untracked());
        });
    }

    /// 'Not now': answers (host memory and tab flag) and takes the banner
    /// away, and opens nothing.
    #[test]
    fn not_now_answers_and_hides_the_banner() {
        Owner::new().with(|| {
            reset_tab_flag();
            let memory = FakeMemory::new(false, true);
            provide_context(Hint::new(memory.clone()));
            let done = RwSignal::new(false);
            not_now(done);
            assert!(done.get_untracked());
            assert_eq!(memory.writes.get(), 1);
            assert!(tab_answered());
        });
    }

    /// F7 table: a sign-in answers the hint only for a signed-in or
    /// remembered account that did not answer yet ('Any sign-in is an
    /// answer, once'; a reload with `pc_learn_hint = 1` re-writes nothing).
    #[test]
    fn a_sign_in_answers_only_an_account_not_yet_answered() {
        let remembered = SyncAccount::Remembered { email: None };
        let signed_in = SyncAccount::SignedIn { email: None };
        let cases = [
            (
                "signed out, not answered",
                SyncAccount::SignedOut,
                false,
                false,
            ),
            ("signed out, answered", SyncAccount::SignedOut, true, false),
            ("remembered, not answered", remembered.clone(), false, true),
            ("remembered, answered", remembered, true, false),
            ("signed in, not answered", signed_in.clone(), false, true),
            ("signed in, answered", signed_in, true, false),
        ];
        for (name, account, answered, expected) in cases {
            assert_eq!(sign_in_answers(&account, answered), expected, "{name}");
        }
    }

    /// F7, host-memory row: the host's memory says seen (a reload), the
    /// tab's flag is clear, the account is signed in: the Effect's body
    /// writes nothing. Does NOT catch the body asking `answered()` after
    /// writing, nor a body that never answers.
    #[test]
    fn a_sign_in_after_a_reload_that_saw_the_hint_writes_nothing() {
        Owner::new().with(|| {
            reset_tab_flag();
            let memory = FakeMemory::new(true, true);
            provide_context(Hint::new(memory.clone()));
            let done = RwSignal::new(false);
            answer_on_sign_in(&SyncAccount::SignedIn { email: None }, done);
            assert_eq!(memory.writes.get(), 0, "the host already knew");
            assert!(!done.get_untracked());
        });
    }

    /// Control of the row above: a memory that says unseen is written once
    /// by a sign-in.
    #[test]
    fn a_first_sign_in_writes_the_host_memory_once() {
        Owner::new().with(|| {
            reset_tab_flag();
            let memory = FakeMemory::new(false, true);
            provide_context(Hint::new(memory.clone()));
            let done = RwSignal::new(false);
            answer_on_sign_in(&SyncAccount::Remembered { email: None }, done);
            assert_eq!(memory.writes.get(), 1);
            assert!(done.get_untracked());
            assert!(tab_answered());
        });
    }

    /// 'Any sign-in is an answer, once': the sign-in Effect's body run twice
    /// with a remembered account (the Effect re-running on the account
    /// signal) writes the host memory once, the second run finding the
    /// tab's flag set. Green on arrival (`answered()` is already asked
    /// before the write). Does NOT catch the body skipping `answered()` and
    /// reading `tab_answered()` alone (the host-memory row above does).
    #[test]
    fn a_sign_in_effect_run_twice_writes_the_host_memory_once() {
        Owner::new().with(|| {
            reset_tab_flag();
            let memory = FakeMemory::new(false, true);
            provide_context(Hint::new(memory.clone()));
            let done = RwSignal::new(false);
            let account = SyncAccount::Remembered { email: None };
            answer_on_sign_in(&account, done);
            answer_on_sign_in(&account, done);
            assert_eq!(memory.writes.get(), 1, "answered once, not twice");
            assert!(done.get_untracked());
        });
    }
}
