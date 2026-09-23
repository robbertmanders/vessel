use super::*;
use crate::firstmate::runs::RunStatus;

/// Recently finished runs stay on the overview this long.
const RECENT_SECONDS: u64 = 24 * 60 * 60;

pub(crate) struct CrewGroups<'a> {
    pub(crate) needs_you: Vec<&'a Run>,
    pub(crate) running: Vec<&'a Run>,
    pub(crate) recent: Vec<&'a Run>,
}

fn mode_order(mode: &str) -> usize {
    match mode {
        "Implement" => 0,
        "Plan" => 1,
        "Review" => 2,
        "Address" => 3,
        "Conflicts" => 4,
        "Description" => 5,
        "Ticket" => 6,
        "Free" => 7,
        "Task" => 8,
        "Scout" => 9,
        _ => 10,
    }
}

/// Groups runs by mode (Remy's order), newest first inside each group.
pub(crate) fn grouped_runs(mut runs: Vec<&Run>) -> Vec<&Run> {
    runs.sort_by(|left, right| {
        mode_order(left.mode())
            .cmp(&mode_order(right.mode()))
            .then_with(|| right.created_at.cmp(&left.created_at))
    });
    runs
}

pub(crate) fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(crate) fn crew_groups<'a>(runs: impl IntoIterator<Item = &'a Run>, now: u64) -> CrewGroups<'a> {
    let runs = runs.into_iter().collect::<Vec<_>>();
    let needs_you = runs
        .iter()
        .copied()
        .filter(|run| run.is_active() && run.status.needs_attention())
        .collect::<Vec<_>>();
    let running = runs
        .iter()
        .copied()
        .filter(|run| run.is_active() && !run.status.needs_attention())
        .collect::<Vec<_>>();
    let recent = runs
        .iter()
        .copied()
        .filter(|run| !run.is_active())
        .filter(|run| {
            run.finished_at
                .or(run.created_at)
                .is_some_and(|at| now.saturating_sub(at) <= RECENT_SECONDS)
        })
        .collect::<Vec<_>>();
    CrewGroups {
        needs_you: grouped_runs(needs_you),
        running: grouped_runs(running),
        recent: grouped_runs(recent),
    }
}

impl App {
    pub(crate) fn crew(&self) -> CrewGroups<'_> {
        let visible = self
            .runs()
            .iter()
            .filter(|run| !self.is_read(run))
            .collect::<Vec<_>>();
        crew_groups(visible, unix_now())
    }

    /// Selectable Crew rows in display order.
    pub(crate) fn crew_runs(&self) -> Vec<&Run> {
        let groups = self.crew();
        groups
            .needs_you
            .into_iter()
            .chain(groups.running)
            .chain(groups.recent)
            .collect()
    }

    pub(crate) fn crew_counts(&self) -> (usize, usize, usize) {
        let live = self
            .runs()
            .iter()
            .filter(|run| run.live && !self.is_read(run));
        let (mut working, mut attention, mut done) = (0, 0, 0);
        for run in live {
            match run.status {
                status if status.needs_attention() => attention += 1,
                RunStatus::Done => done += 1,
                _ => working += 1,
            }
        }
        (working, attention, done)
    }

    pub(crate) fn move_overview_selection(&mut self, offset: isize) {
        self.overview_selected = self
            .overview_selected
            .saturating_add_signed(offset)
            .min(self.crew_runs().len().saturating_sub(1));
    }

    pub(crate) fn scroll_overview(&mut self, offset: isize) {
        self.overview_scroll_target = None;
        self.overview_scroll = self.overview_scroll.saturating_add_signed(offset);
    }

    pub(crate) fn move_overview_section(&mut self, offset: isize) {
        let current = match self.overview_section {
            OverviewSection::Crew => 0,
            OverviewSection::Jira => 1,
            OverviewSection::GitHubMe => 2,
            OverviewSection::GitHubOther => 3,
        };
        self.overview_section = match (current as isize + offset).rem_euclid(4) {
            0 => OverviewSection::Crew,
            1 => OverviewSection::Jira,
            2 => {
                self.github_section = GitHubSection::MyWork;
                OverviewSection::GitHubMe
            }
            _ => {
                self.github_section = GitHubSection::OtherWork;
                OverviewSection::GitHubOther
            }
        };
        self.overview_scroll_target = Some(self.overview_section);
    }

    pub(crate) fn select_overview_section(&mut self, section: OverviewSection) {
        self.select_tab(0);
        self.overview_section = section;
        self.overview_scroll_target = Some(section);
        match section {
            OverviewSection::GitHubMe => self.github_section = GitHubSection::MyWork,
            OverviewSection::GitHubOther => self.github_section = GitHubSection::OtherWork,
            OverviewSection::Jira | OverviewSection::Crew => {}
        }
    }

    pub(crate) fn open_selected_overview_run(&mut self, details: bool) {
        let Some((task, created_at)) = self
            .crew_runs()
            .get(self.overview_selected)
            .map(|run| (run.task.clone(), run.created_at))
        else {
            return;
        };
        self.enter_run(task, created_at, details);
    }

    pub(crate) fn open_activity(&mut self) {
        self.activity = Some(ActivityState::default());
    }

    /// Storage key for one run: task plus dispatch time, so two runs of the
    /// same task are marked independently.
    pub(crate) fn read_key(task: &str, created_at: Option<u64>) -> String {
        format!("{task}@{}", created_at.unwrap_or(0))
    }

    pub(crate) fn is_read(&self, run: &Run) -> bool {
        self.read_runs
            .contains(&Self::read_key(&run.task, run.created_at))
    }

    /// Marks the selected overview run read: it is persisted and disappears
    /// from the overview. Other runs are untouched.
    pub(crate) fn mark_selected_overview_run_read(&mut self) {
        let selected = self
            .crew_runs()
            .get(self.overview_selected)
            .map(|run| (run.task.clone(), run.created_at));
        let Some((task, created_at)) = selected else {
            return;
        };
        let key = Self::read_key(&task, created_at);
        if self.read_runs.insert(key)
            && let Err(message) = save_read_runs(&self.config, &self.read_runs)
        {
            self.notice = Some(message);
            return;
        }
        self.overview_selected = self
            .overview_selected
            .min(self.crew_runs().len().saturating_sub(1));
        self.notice = Some(format!("Marked {task} read"));
    }

    pub(crate) fn close_activity(&mut self) {
        self.activity = None;
    }

    pub(crate) fn scroll_activity(&mut self, offset: i16) {
        if let Some(activity) = &mut self.activity {
            activity.scroll = activity.scroll.saturating_add_signed(offset);
        }
    }
}

/// Read keys persisted at `config/vessel/read.json`. A missing or corrupt
/// file means nothing is read yet; vessel never blocks on it.
pub(crate) fn load_read_runs(config: &Config) -> BTreeSet<String> {
    let Ok(text) = std::fs::read_to_string(config.read_runs_file()) else {
        return BTreeSet::new();
    };
    serde_json::from_str::<Vec<String>>(&text)
        .map(BTreeSet::from_iter)
        .unwrap_or_default()
}

fn save_read_runs(config: &Config, read: &BTreeSet<String>) -> Result<(), String> {
    let mut keys = read.iter().collect::<Vec<_>>();
    keys.sort();
    let contents = serde_json::to_string_pretty(&keys)
        .map(|json| format!("{json}\n"))
        .map_err(|error| format!("Could not save read sessions: {error}"))?;
    let path = config.read_runs_file();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Could not write {}: {error}", path.display()))?;
    }
    crate::config::write_atomic(&path, &contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::firstmate::runs::TimelineEntry;

    fn run(task: &str, workflow: &str, live: bool, status: RunStatus, at: u64) -> Run {
        Run {
            task: task.into(),
            workflow: Some(workflow.into()),
            record: None,
            kind: None,
            project: None,
            harness: None,
            model: None,
            title: task.into(),
            ticket_key: None,
            repo: None,
            pr_number: None,
            pr_url: None,
            created_at: Some(at),
            finished_at: (!live).then_some(at + 10),
            status,
            status_text: None,
            timeline: Vec::<TimelineEntry>::new(),
            open_decisions: Vec::new(),
            live,
            endpoint_exists: None,
        }
    }

    #[test]
    fn crew_separates_attention_running_and_recent_runs() {
        let now = 1_000_000;
        let runs = vec![
            run("review-a", "review", true, RunStatus::Working, now - 50),
            run(
                "implement-b",
                "implement",
                true,
                RunStatus::Working,
                now - 60,
            ),
            run("plan-c", "plan", true, RunStatus::NeedsDecision, now - 70),
            run("review-d", "review", false, RunStatus::Completed, now - 100),
            run(
                "review-old",
                "review",
                false,
                RunStatus::Completed,
                now - 200_000,
            ),
        ];

        let groups = crew_groups(&runs, now);

        assert_eq!(
            groups
                .needs_you
                .iter()
                .map(|run| run.task.as_str())
                .collect::<Vec<_>>(),
            ["plan-c"]
        );
        assert_eq!(
            groups
                .running
                .iter()
                .map(|run| run.task.as_str())
                .collect::<Vec<_>>(),
            ["implement-b", "review-a"]
        );
        assert_eq!(
            groups
                .recent
                .iter()
                .map(|run| run.task.as_str())
                .collect::<Vec<_>>(),
            ["review-d"]
        );
    }

    #[test]
    fn a_scout_that_reported_done_is_finished_before_cleanup() {
        let now = 1_000_000;
        let mut review = run("review-a", "review", true, RunStatus::Done, now - 50);
        review.kind = Some("scout".into());
        let mut ship = run("implement-b", "implement", true, RunStatus::Done, now - 60);
        ship.kind = Some("ship".into());
        let runs = vec![review, ship];

        let groups = crew_groups(&runs, now);

        let tasks = |runs: &[&Run]| runs.iter().map(|run| run.task.clone()).collect::<Vec<_>>();
        assert_eq!(tasks(&groups.recent), ["review-a"]);
        assert_eq!(
            tasks(&groups.running),
            ["implement-b"],
            "a ship's done may still await its PR"
        );
    }

    fn read_test_app(name: &str, runs: Vec<Run>) -> (App, std::path::PathBuf) {
        let dir = std::env::temp_dir().join(format!("vessel-read-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut app = App::default();
        app.config = Config {
            fm_home: dir.clone(),
            fm_root: dir.clone(),
            ..Config::default()
        };
        app.fleet.runs = runs;
        (app, dir)
    }

    fn live_run(task: &str, at: u64) -> Run {
        run(task, "implement", true, RunStatus::Working, at)
    }

    #[test]
    fn marking_selected_run_read_hides_it_from_overview() {
        let now = unix_now();
        let (mut app, dir) = read_test_app(
            "hide",
            vec![live_run("task-a", now - 50), live_run("task-b", now - 60)],
        );
        assert_eq!(app.crew_runs().len(), 2);

        app.mark_selected_overview_run_read();

        let tasks = app
            .crew_runs()
            .iter()
            .map(|run| run.task.clone())
            .collect::<Vec<_>>();
        assert_eq!(tasks.len(), 1, "only the marked run disappears");
        assert!(
            app.is_read(&live_run(
                tasks.first().map_or("task-a", String::as_str),
                now - 50
            )) || tasks == ["task-b"],
            "the unmarked run stays visible: {tasks:?}"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn read_state_is_keyed_by_task_and_dispatch_time() {
        let now = unix_now();
        let first = live_run("task-a", now - 50);
        let second = Run {
            created_at: Some(now - 40),
            ..live_run("task-a", now - 50)
        };
        let (mut app, dir) = read_test_app("keyed", vec![first, second]);
        assert_eq!(app.crew_runs().len(), 2);

        app.mark_selected_overview_run_read();

        assert_eq!(
            app.crew_runs().len(),
            1,
            "the other dispatch of the same task stays visible"
        );
        assert_eq!(app.read_runs.len(), 1);
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn read_state_persists_across_loads() {
        let now = unix_now();
        let (mut app, dir) = read_test_app("persist", vec![live_run("task-a", now - 50)]);

        app.mark_selected_overview_run_read();
        assert!(app.crew_runs().is_empty());

        let reloaded = load_read_runs(&app.config);
        assert_eq!(reloaded, app.read_runs);
        assert!(reloaded.contains(&App::read_key("task-a", Some(now - 50))));

        let mut fresh = App::default();
        fresh.config = app.config.clone();
        fresh.fleet.runs = vec![live_run("task-a", now - 50)];
        fresh.read_runs = reloaded;
        assert!(
            fresh.crew_runs().is_empty(),
            "a reloaded home still hides the read run"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn missing_or_corrupt_read_file_means_nothing_read() {
        let dir = std::env::temp_dir().join(format!("vessel-read-{}-absent", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let config = Config {
            fm_home: dir.clone(),
            fm_root: dir.clone(),
            ..Config::default()
        };
        assert!(load_read_runs(&config).is_empty());
        std::fs::create_dir_all(config.vessel_dir()).unwrap();
        std::fs::write(config.read_runs_file(), "not json").unwrap();
        assert!(load_read_runs(&config).is_empty());
        std::fs::remove_dir_all(dir).ok();
    }
}
