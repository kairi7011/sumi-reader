use std::time::{Duration, Instant};

pub const FEEDBACK_DELAY: Duration = Duration::from_millis(400);

#[derive(Default)]
pub struct Activity {
    started: Option<Instant>,
}

impl Activity {
    pub fn begin(&mut self) { self.started = Some(Instant::now()); }
    pub fn finish(&mut self) { self.started = None; }
    pub fn set_active(&mut self, active: bool) {
        if !active { self.finish(); }
        else if self.started.is_none() { self.begin(); }
    }
    pub fn visible(&self) -> bool { self.visible_at(Instant::now()) }
    fn visible_at(&self, now: Instant) -> bool {
        self.started.is_some_and(|started| now.saturating_duration_since(started) >= FEEDBACK_DELAY)
    }
}

#[cfg(debug_assertions)]
pub fn trace_event(event: &str) {
    use std::{fs::OpenOptions, io::Write, os::unix::fs::OpenOptionsExt, path::PathBuf, time::{SystemTime, UNIX_EPOCH}};
    if std::env::var("SUMI_NATIVE_TRACE").ok().as_deref()!=Some("1"){return;}
    let Some(library)=std::env::var_os("SUMI_LIBRARY_PATH").map(PathBuf::from)else{return;};
    if !library.is_absolute()||!library.components().any(|part|part.as_os_str()=="test-artifacts"){return;}
    let Some(parent)=library.parent()else{return;};
    let Ok(mut file)=OpenOptions::new().create(true).append(true).mode(0o600).open(parent.join("timing.jsonl"))else{return;};
    let time=SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs_f64();
    let _=writeln!(file,"{}",serde_json::json!({"event":event,"time":time}));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feedback_waits_four_hundred_milliseconds_and_stops_on_completion() {
        let now = Instant::now();
        let mut activity = Activity { started: Some(now) };
        assert!(!activity.visible_at(now + Duration::from_millis(399)));
        assert!(activity.visible_at(now + Duration::from_millis(400)));
        assert!(activity.visible_at(now + Duration::from_secs(2)));
        activity.finish();
        assert!(!activity.visible_at(now + Duration::from_secs(3)));
    }

    #[test]
    fn continuing_work_keeps_its_deadline_but_cancelled_work_does_not() {
        let before = Instant::now() - Duration::from_secs(1);
        let mut activity = Activity { started: Some(before) };
        activity.set_active(true);
        assert_eq!(activity.started, Some(before));
        assert!(activity.visible());
        activity.set_active(false);
        activity.set_active(true);
        assert!(!activity.visible());
        activity.started = Some(before);
        activity.begin();
        assert!(!activity.visible());
    }
}
