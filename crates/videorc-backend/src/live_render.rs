//! LS2: the live render consumer (fake-source proof).
//!
//! Proves the architecture the live-editing feature depends on: an output that
//! consumes committed scene revisions *continuously*, so a hot transform/visibility/
//! order edit changes the next rendered frame **without restarting the output**. It
//! uses fake sources (each scene source paints a deterministic solid colour) and a
//! tiny painter's-algorithm compositor, so the loop is provable in deterministic
//! tests without real capture or a real encoder. The real recording/stream output
//! becomes a consumer of these frames in LS3+, hence `allow(dead_code)` for now.
#![allow(dead_code)]

use crate::live_scene::{
    ActiveScene, LiveEditDecision, LiveEditEvent, MutationContext, SceneMutation, decode_op,
};
use crate::protocol::{SceneSource, SceneTransform};

/// The background colour where no visible source covers a pixel.
const BACKGROUND: [u8; 3] = [0, 0, 0];

/// A composited frame: the colour of the topmost visible source at each pixel.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderedFrame {
    pub revision: u64,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 3]>,
}

impl RenderedFrame {
    pub fn pixel(&self, x: usize, y: usize) -> [u8; 3] {
        self.pixels[y * self.width + x]
    }

    /// Samples the colour at a normalized (0..1) point.
    pub fn sample(&self, nx: f64, ny: f64) -> [u8; 3] {
        let x = ((nx * self.width as f64) as usize).min(self.width.saturating_sub(1));
        let y = ((ny * self.height as f64) as usize).min(self.height.saturating_sub(1));
        self.pixel(x, y)
    }

    /// Flattened RGB bytes, ready to feed a rawvideo encoder (LS3).
    pub fn rgb_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(self.pixels.len() * 3);
        for px in &self.pixels {
            bytes.extend_from_slice(px);
        }
        bytes
    }
}

/// A stable, distinct-ish colour for a source id, so tests can predict what a source
/// paints. The high bit per channel is forced on so no source is mistaken for the
/// black background.
pub fn source_color(id: &str) -> [u8; 3] {
    let mut hash: u32 = 2_166_136_261;
    for byte in id.bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(16_777_619);
    }
    [
        ((hash >> 16) as u8) | 0x80,
        ((hash >> 8) as u8) | 0x80,
        (hash as u8) | 0x80,
    ]
}

fn covers(transform: &SceneTransform, x: f64, y: f64) -> bool {
    x >= transform.x
        && x < transform.x + transform.width
        && y >= transform.y
        && y < transform.y + transform.height
}

/// Composites the scene into a frame. Sources are painted in list order (painter's
/// algorithm), so the last visible source covering a pixel wins — reordering a source
/// to the end brings it to the top.
pub fn composite(
    sources: &[SceneSource],
    revision: u64,
    width: usize,
    height: usize,
) -> RenderedFrame {
    let mut pixels = vec![BACKGROUND; width * height];
    for py in 0..height {
        let cy = (py as f64 + 0.5) / height as f64;
        for px in 0..width {
            let cx = (px as f64 + 0.5) / width as f64;
            for source in sources {
                if source.visible && covers(&source.transform, cx, cy) {
                    pixels[py * width + px] = source_color(&source.id);
                }
            }
        }
    }
    RenderedFrame {
        revision,
        width,
        height,
        pixels,
    }
}

/// An output that consumes committed scene revisions continuously. Submitting a hot
/// mutation changes the *next* rendered frame; the consumer is never recreated for hot
/// changes, so its `generation` stays constant (a true restart — a cold output-mode
/// change in LS7 — would bump it).
#[derive(Debug, Clone)]
pub struct LiveRenderConsumer {
    scene: ActiveScene,
    width: usize,
    height: usize,
    generation: u64,
    frames_rendered: u64,
}

impl LiveRenderConsumer {
    pub fn start(scene: ActiveScene, width: usize, height: usize) -> Self {
        Self {
            scene,
            width,
            height,
            generation: 1,
            frames_rendered: 0,
        }
    }

    /// Submits a live mutation: the contract validates/classifies/logs it, then a
    /// committed hot mutation is executed so the next frame reflects it. The output is
    /// not restarted.
    pub fn submit(
        &mut self,
        mutation: &SceneMutation,
        ctx: &MutationContext,
        now: &str,
    ) -> LiveEditDecision {
        let decision = self.scene.apply(mutation, ctx, now);
        if decision.committed
            && let Some(op) = decode_op(mutation)
        {
            self.scene.execute_op(&op);
        }
        decision
    }

    /// Renders the next frame from the current committed scene.
    pub fn render_next(&mut self) -> RenderedFrame {
        self.frames_rendered += 1;
        composite(
            self.scene.sources(),
            self.scene.revision(),
            self.width,
            self.height,
        )
    }

    pub fn revision(&self) -> u64 {
        self.scene.revision()
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn frames_rendered(&self) -> u64 {
        self.frames_rendered
    }

    pub fn timeline(&self) -> &[LiveEditEvent] {
        self.scene.events()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_scene::{ActiveSceneState, MutationKind, SessionMode};
    use crate::protocol::{
        SceneOutputKind, SceneSourceKind, SceneTransform, default_layout_settings,
    };

    fn transform(x: f64, y: f64, width: f64, height: f64) -> SceneTransform {
        SceneTransform {
            x,
            y,
            width,
            height,
            crop_left: 0.0,
            crop_top: 0.0,
            crop_right: 0.0,
            crop_bottom: 0.0,
        }
    }

    fn source(id: &str, kind: SceneSourceKind, t: SceneTransform, visible: bool) -> SceneSource {
        SceneSource {
            id: id.to_string(),
            name: id.to_string(),
            kind,
            device_id: None,
            transform: t.clone(),
            default_transform: t,
            visible,
            locked: false,
        }
    }

    /// A screen + camera scene: full-frame screen with a small bottom-right camera.
    fn screen_camera_scene() -> ActiveScene {
        ActiveScene::new(ActiveSceneState {
            session_id: "session-1".to_string(),
            scene_id: "scene:default".to_string(),
            revision: 0,
            layout: default_layout_settings(),
            sources: vec![
                source(
                    "source:base",
                    SceneSourceKind::Screen,
                    transform(0.0, 0.0, 1.0, 1.0),
                    true,
                ),
                source(
                    "source:camera",
                    SceneSourceKind::Camera,
                    transform(0.6, 0.6, 0.3, 0.3),
                    true,
                ),
            ],
            outputs: vec![SceneOutputKind::Recording],
            mode: SessionMode::Recording,
            updated_at: "t0".to_string(),
        })
    }

    fn screen_camera_consumer() -> LiveRenderConsumer {
        LiveRenderConsumer::start(screen_camera_scene(), 32, 32)
    }

    fn transform_mutation(id: &str, expected_revision: u64, x: f64, y: f64) -> SceneMutation {
        SceneMutation {
            id: id.to_string(),
            expected_revision,
            kind: MutationKind::SourceTransformPatch,
            apply_mode: None,
            payload: serde_json::json!({
                "sourceId": "source:camera",
                "transform": { "x": x, "y": y, "width": 0.3, "height": 0.3 },
            }),
            created_at: "t".to_string(),
        }
    }

    #[test]
    fn composite_paints_topmost_visible_source() {
        let consumer = screen_camera_consumer();
        let frame = composite(consumer.scene.sources(), 0, 32, 32);
        // Camera (on top) wins inside its rect; screen wins elsewhere.
        assert_eq!(frame.sample(0.75, 0.75), source_color("source:camera"));
        assert_eq!(frame.sample(0.5, 0.5), source_color("source:base"));
    }

    #[test]
    fn moving_a_source_changes_the_next_frame_without_restart() {
        let mut consumer = screen_camera_consumer();
        let before = consumer.render_next();
        assert_eq!(before.sample(0.75, 0.75), source_color("source:camera"));
        assert_eq!(before.revision, 0);

        // Move the camera to the top-left corner.
        let decision = consumer.submit(
            &transform_mutation("m1", 0, 0.0, 0.0),
            &MutationContext::default(),
            "t1",
        );
        assert!(decision.committed);

        let after = consumer.render_next();
        assert_eq!(after.revision, 1, "the committed revision advanced");
        // Camera now paints the top-left and has left the bottom-right.
        assert_eq!(after.sample(0.1, 0.1), source_color("source:camera"));
        assert_eq!(after.sample(0.75, 0.75), source_color("source:base"));

        // Same consumer instance, no restart.
        assert_eq!(consumer.generation(), 1);
        assert_eq!(consumer.frames_rendered(), 2);
    }

    #[test]
    fn hiding_a_source_removes_it_from_the_frame() {
        let mut consumer = screen_camera_consumer();
        consumer.submit(
            &SceneMutation {
                id: "hide".to_string(),
                expected_revision: 0,
                kind: MutationKind::SourceVisibilitySet,
                apply_mode: None,
                payload: serde_json::json!({ "sourceId": "source:camera", "visible": false }),
                created_at: "t".to_string(),
            },
            &MutationContext::default(),
            "t1",
        );
        let frame = consumer.render_next();
        // Where the camera was now shows the screen beneath it.
        assert_eq!(frame.sample(0.75, 0.75), source_color("source:base"));
        assert_eq!(consumer.generation(), 1);
    }

    #[test]
    fn reordering_changes_which_source_is_on_top() {
        // Two fully-overlapping sources; the later one wins.
        let scene = ActiveScene::new(ActiveSceneState {
            session_id: "session-1".to_string(),
            scene_id: "scene:default".to_string(),
            revision: 0,
            layout: default_layout_settings(),
            sources: vec![
                source(
                    "a",
                    SceneSourceKind::Screen,
                    transform(0.0, 0.0, 1.0, 1.0),
                    true,
                ),
                source(
                    "b",
                    SceneSourceKind::Window,
                    transform(0.0, 0.0, 1.0, 1.0),
                    true,
                ),
            ],
            outputs: vec![SceneOutputKind::Recording],
            mode: SessionMode::Streaming,
            updated_at: "t0".to_string(),
        });
        let mut consumer = LiveRenderConsumer::start(scene, 8, 8);
        assert_eq!(consumer.render_next().sample(0.5, 0.5), source_color("b"));

        // Reorder so "a" is last (on top).
        consumer.submit(
            &SceneMutation {
                id: "reorder".to_string(),
                expected_revision: 0,
                kind: MutationKind::SourceOrderSet,
                apply_mode: None,
                payload: serde_json::json!({ "sourceIds": ["b", "a"] }),
                created_at: "t".to_string(),
            },
            &MutationContext::default(),
            "t1",
        );
        assert_eq!(consumer.render_next().sample(0.5, 0.5), source_color("a"));
        assert_eq!(consumer.generation(), 1);
    }

    #[test]
    fn timeline_records_every_live_edit() {
        let mut consumer = screen_camera_consumer();
        consumer.submit(
            &transform_mutation("m1", 0, 0.0, 0.0),
            &MutationContext::default(),
            "t1",
        );
        consumer.submit(
            &transform_mutation("m2", 1, 0.3, 0.3),
            &MutationContext::default(),
            "t2",
        );
        assert_eq!(consumer.timeline().len(), 2);
        assert!(
            consumer
                .timeline()
                .iter()
                .all(|e| e.session_id == "session-1")
        );
    }

    #[test]
    fn stale_edit_does_not_change_the_frame() {
        let mut consumer = screen_camera_consumer();
        consumer.submit(
            &transform_mutation("m1", 0, 0.0, 0.0),
            &MutationContext::default(),
            "t1",
        );
        // A second edit still expecting revision 0 is stale and is ignored.
        let decision = consumer.submit(
            &transform_mutation("m2", 0, 0.5, 0.5),
            &MutationContext::default(),
            "t2",
        );
        assert!(!decision.accepted);
        let frame = consumer.render_next();
        // Camera stayed where the first (accepted) edit put it: top-left.
        assert_eq!(frame.sample(0.1, 0.1), source_color("source:camera"));
        assert_eq!(frame.revision, 1);
    }

    /// End-to-end render → encode proof (the architectural crux of live editing):
    /// render a screen+camera scene, move the camera halfway through, and pipe the
    /// rgb24 frames to ffmpeg as rawvideo. Confirms a real encoder consumes the live
    /// render output continuously and the recording finalizes, without restart.
    /// Ignored by default (spawns ffmpeg + writes a file); run with `--ignored`.
    #[test]
    #[ignore = "spawns ffmpeg and writes a file; run with --ignored"]
    fn fake_recording_encodes_a_moving_source() {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let (width, height, fps, total_frames) = (320usize, 180usize, 30usize, 60usize);
        let output = std::env::temp_dir().join("videorc-ls2-fake-recording.mkv");
        let _ = std::fs::remove_file(&output);

        let mut child = Command::new("ffmpeg")
            .args([
                "-y",
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-s",
                &format!("{width}x{height}"),
                "-r",
                &fps.to_string(),
                "-i",
                "pipe:0",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-pix_fmt",
                "yuv420p",
            ])
            .arg(&output)
            .stdin(Stdio::piped())
            .spawn()
            .expect("ffmpeg should be on PATH for this ignored test");

        let mut stdin = child.stdin.take().expect("ffmpeg stdin");
        let mut consumer = LiveRenderConsumer::start(screen_camera_scene(), width, height);
        for frame_index in 0..total_frames {
            if frame_index == total_frames / 2 {
                // Move the camera to the top-left mid-recording, without restarting.
                consumer.submit(
                    &transform_mutation("move", 0, 0.0, 0.0),
                    &MutationContext::default(),
                    "mid",
                );
            }
            let frame = consumer.render_next();
            stdin.write_all(&frame.rgb_bytes()).expect("write frame");
        }
        drop(stdin);

        let status = child.wait().expect("ffmpeg wait");
        assert!(status.success(), "ffmpeg should finalize the recording");
        let size = std::fs::metadata(&output).expect("recording exists").len();
        assert!(size > 0, "the fake recording should contain encoded video");
        assert_eq!(consumer.generation(), 1, "the output never restarted");
    }
}
