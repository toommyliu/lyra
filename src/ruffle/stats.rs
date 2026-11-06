use std::{num::NonZeroUsize, time::Duration};

use ruffle_core::{Player, ViewportDimensions, stats::StatsSample};
use tokio::sync::watch;

pub(crate) const GRAPH_SAMPLES: usize = 64;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Frame {
    pub(crate) logic_ms: f64,
    pub(crate) render_ms: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Timing {
    pub(crate) average_ms: f64,
    pub(crate) min_ms: f64,
    pub(crate) max_ms: f64,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Report {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) game_fps: Option<f64>,
    pub(crate) target_fps: f64,
    pub(crate) logic: Option<Timing>,
    pub(crate) render: Option<Timing>,
    frames: [Frame; GRAPH_SAMPLES],
    frame_count: usize,
    next_frame: usize,
}

impl Report {
    pub(crate) fn frames(&self) -> impl ExactSizeIterator<Item = Frame> + '_ {
        (0..self.frame_count).map(|offset| {
            self.frames
                [(self.next_frame + GRAPH_SAMPLES - self.frame_count + offset) % GRAPH_SAMPLES]
        })
    }
}

#[derive(Default)]
struct Durations {
    sum: f64,
    min: f64,
    max: f64,
    count: usize,
}

impl Durations {
    fn add(&mut self, ms: f64) {
        self.sum += ms;
        self.min = if self.count == 0 {
            ms
        } else {
            self.min.min(ms)
        };
        self.max = self.max.max(ms);
        self.count += 1;
    }

    fn take(&mut self) -> Option<Timing> {
        let values = std::mem::take(self);
        (values.count > 0).then(|| Timing {
            average_ms: values.sum / values.count as f64,
            min_ms: values.min,
            max_ms: values.max,
        })
    }
}

struct Session {
    report: Report,
    window_start: StatsSample,
    logic: Durations,
    render: Durations,
    pending_logic_ms: f64,
}

impl Session {
    fn record(&mut self, sample: StatsSample) -> bool {
        self.report.target_fps = sample.target_frame_rate;
        self.pending_logic_ms += sample.tick_time.as_secs_f64() * 1000.0;
        if let Some(render_time) = sample.render_time {
            let render_ms = render_time.as_secs_f64() * 1000.0;
            self.logic.add(self.pending_logic_ms);
            self.render.add(render_ms);
            self.report.frames[self.report.next_frame] = Frame {
                logic_ms: std::mem::take(&mut self.pending_logic_ms),
                render_ms,
            };
            self.report.next_frame = (self.report.next_frame + 1) % GRAPH_SAMPLES;
            self.report.frame_count = (self.report.frame_count + 1).min(GRAPH_SAMPLES);
        }
        let elapsed = sample.timestamp - self.window_start.timestamp;
        let text_due = elapsed >= Duration::from_millis(500);
        if text_due {
            self.report.game_fps = Some(
                (sample.game_frames - self.window_start.game_frames) as f64 / elapsed.as_secs_f64(),
            );
            self.report.logic = self.logic.take();
            self.report.render = self.render.take();
            self.window_start = sample;
        }
        sample.render_time.is_some() || text_due
    }
}

pub(super) struct Collector {
    reports: watch::Sender<Option<Report>>,
    session: Option<Session>,
}

impl Collector {
    pub(super) fn new(reports: watch::Sender<Option<Report>>) -> Self {
        Self {
            reports,
            session: None,
        }
    }

    pub(super) fn enabled(&self) -> bool {
        self.session.is_some()
    }

    pub(super) fn set_enabled(&mut self, player: &mut Player, enabled: bool) {
        if self.enabled() == enabled {
            return;
        }
        if enabled {
            player.enable_stats(NonZeroUsize::new(1).unwrap());
            let baseline = player.sample_stats(None).unwrap();
            player.drain_stats().for_each(drop);
            let viewport = player.viewport_dimensions();
            let report = Report {
                width: viewport.width,
                height: viewport.height,
                game_fps: None,
                target_fps: baseline.target_frame_rate,
                logic: None,
                render: None,
                frames: [Frame::default(); GRAPH_SAMPLES],
                frame_count: 0,
                next_frame: 0,
            };
            self.reports.send_replace(Some(report));
            self.session = Some(Session {
                report,
                window_start: baseline,
                logic: Durations::default(),
                render: Durations::default(),
                pending_logic_ms: 0.0,
            });
        } else {
            player.disable_stats();
            self.session = None;
            self.reports.send_replace(None);
        }
    }

    pub(super) fn sample(&mut self, player: &mut Player, viewport: ViewportDimensions) {
        let Some(session) = &mut self.session else {
            return;
        };
        session.report.width = viewport.width;
        session.report.height = viewport.height;
        let sample = player.sample_stats(None).unwrap();
        player.drain_stats().for_each(drop);
        if session.record(sample) {
            self.reports.send_replace(Some(session.report));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_is_opt_in_immediate_and_restarts_with_fresh_counters() {
        let (reports, mut receiver) = watch::channel(None);
        let mut collector = Collector::new(reports);
        let player = ruffle_core::PlayerBuilder::new().build();
        let mut player = player.lock().unwrap();
        let viewport = player.viewport_dimensions();
        collector.sample(&mut player, viewport);
        assert!(!player.stats_enabled());
        assert!(receiver.borrow().is_none());
        collector.set_enabled(&mut player, true);
        assert!(receiver.borrow_and_update().is_some());
        player.run_frame();
        player.render();
        collector.sample(&mut player, viewport);
        collector.set_enabled(&mut player, true);
        assert_eq!(player.sample_stats(None).unwrap().game_frames, 1);
        collector.set_enabled(&mut player, false);
        assert!(!player.stats_enabled());
        assert!(receiver.borrow_and_update().is_none());
        collector.set_enabled(&mut player, true);
        assert_eq!(receiver.borrow_and_update().unwrap().frame_count, 0);
        assert_eq!(player.sample_stats(None).unwrap().game_frames, 0);
    }

    #[test]
    fn reports_cpu_ranges_and_game_fps_at_both_render_cadences() {
        for fps in [24, 60, 144] {
            let (reports, _) = watch::channel(None);
            let mut collector = Collector::new(reports);
            let player = ruffle_core::PlayerBuilder::new().build();
            let mut player = player.lock().unwrap();
            collector.set_enabled(&mut player, true);
            let session = collector.session.as_mut().unwrap();
            let baseline = session.window_start;
            for frame in 1..=fps * 2 {
                assert!(session.record(StatsSample {
                    timestamp: baseline.timestamp
                        + Duration::from_secs_f64(frame as f64 / fps as f64),
                    tick_time: Duration::from_millis(if frame % 2 == 0 { 3 } else { 1 }),
                    render_time: Some(Duration::from_millis(1)),
                    game_frames: frame * 24 / fps,
                    renders: frame,
                    target_frame_rate: 24.0,
                    ..baseline
                }));
            }
            let report = session.report;
            assert_eq!(report.game_fps, Some(24.0));
            assert_eq!(report.logic.unwrap().average_ms, 2.0);
            assert_eq!(report.logic.unwrap().min_ms, 1.0);
            assert_eq!(report.logic.unwrap().max_ms, 3.0);
            assert_eq!(report.render.unwrap().average_ms, 1.0);
            assert_eq!(report.frames().len(), (fps as usize * 2).min(GRAPH_SAMPLES));
        }
    }
}
