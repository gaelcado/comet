//! Central voice surface occupies the composer's shell slot. No editor entity
//! is mounted here; retaining the composer elsewhere preserves its draft.
use gpui::{prelude::*, Context, Entity, FocusHandle, Render, Window, div, px};
use zeron_proto::voice::{VoicePhase,VoiceSnapshot,VoiceWork};
use crate::{orb::{Orb,OrbSize,OrbState}, theme::Theme, voice::VoiceController,composer::Composer};

pub fn orb_state(phase:VoicePhase,snapshot:Option<&VoiceSnapshot>) -> OrbState {
    if matches!(phase,VoicePhase::Starting|VoicePhase::Stopping) { return OrbState::Connecting; }
    match snapshot {
        Some(s) if s.work==VoiceWork::Working=>OrbState::Working,
        Some(s) if s.playing=>OrbState::Composing,
        Some(s) if s.muted=>OrbState::Breathing,
        Some(_)=>OrbState::Listening,
        None=>OrbState::Breathing,
    }
}
pub struct VoiceSurface {
    controller:Entity<VoiceController>,
    composer:Entity<Composer>,
    orb:Entity<Orb>,
    focus:FocusHandle,
    mute_focus:FocusHandle,
    end_focus:FocusHandle,
    task_focus:FocusHandle,
}
impl VoiceSurface {
    pub fn new(controller:Entity<VoiceController>,composer:Entity<Composer>,cx:&mut Context<Self>) -> Self {
        let orb=cx.new(|_|Orb::new().size(OrbSize::Hero).visible(false));
        Self { controller,composer,orb,focus:cx.focus_handle(),mute_focus:cx.focus_handle().tab_stop(true),end_focus:cx.focus_handle().tab_stop(true),task_focus:cx.focus_handle().tab_stop(true) }
    }
    pub fn set_visible(&mut self,visible:bool,cx:&mut Context<Self>) {
        self.orb.update(cx,|orb,cx|orb.set_visible(visible,cx));
    }
    pub fn focus_handle(&self) -> FocusHandle { self.end_focus.clone() }
}
impl Render for VoiceSurface {
    fn render(&mut self,window:&mut Window,cx:&mut Context<Self>) -> impl IntoElement {
        let controller=self.controller.read(cx);
        let phase=controller.phase;
        let partial=controller.partial.clone();
        let snapshot=controller.snapshot.clone();
        let muted=snapshot.as_ref().is_some_and(|s|s.muted);
        let working=snapshot.as_ref().is_some_and(|s|s.work!=VoiceWork::Idle);
        let state=orb_state(phase,snapshot.as_ref());
        let label=match phase {
            VoicePhase::Starting=>"Connecting…",VoicePhase::Stopping=>"Ending voice…",
            _ if muted=>"Microphone muted",_ if working=>"Codex is working",_=>"Listening",
        };
        let reduced=crate::motion::reduced_motion(cx);
        self.orb.update(cx,|orb,cx| {
            orb.set_visible(phase.replaces_composer(),cx);
            orb.set_state(state,cx);
            orb.set_size(if f32::from(window.viewport_size().width)<440.0 { OrbSize::Large } else { OrbSize::Hero },cx);
            orb.set_reduced_motion(reduced,cx);
        });
        let theme=Theme::of(cx).clone();
        let controller=self.controller.clone(); let mute_controller=controller.clone();
        let composer=self.composer.clone();
        div().id("voice-surface").track_focus(&self.focus).w_full().py(px(16.0))
            .flex().flex_col().items_center().gap(px(10.0)).text_color(theme.text)
            .on_key_down(cx.listener(|this,event: &gpui::KeyDownEvent,window,cx| {
                if event.keystroke.key=="escape" { this.controller.update(cx,|voice,cx|voice.cancel(cx)); cx.stop_propagation(); }
                else if event.keystroke.key=="enter" || event.keystroke.key=="space" {
                    if this.end_focus.is_focused(window) { this.controller.update(cx,|voice,cx|voice.cancel(cx)); }
                    else if this.mute_focus.is_focused(window) { this.controller.update(cx,|voice,cx|voice.toggle_mute(cx)); }
                    else if this.task_focus.is_focused(window) { this.composer.update(cx,|composer,cx|composer.interrupt_selected(cx)); }
                    cx.stop_propagation();
                }
            }))
            .child(self.orb.clone())
            .child(div().text_sm().child(label))
            .when(snapshot.as_ref().is_some_and(|s|s.playing),|el|el.child(div().text_xs().child("Playing response")))
            .when(!partial.is_empty(),|el|el.child(div().max_w(px(500.0)).text_sm().child(partial)))
            .child(div().flex().items_center().gap(px(16.0))
                .child(div().id("voice-mute").track_focus(&self.mute_focus).cursor_pointer().px(px(12.0)).py(px(6.0)).rounded_full()
                    .tooltip(crate::settings::widgets::text_tooltip(if muted {"Unmute microphone"} else {"Mute microphone"}))
                    .on_click(move |_,_,cx|mute_controller.update(cx,|voice,cx|voice.toggle_mute(cx)))
                    .child(if muted {"Unmute"} else {"Mute"}))
                .child(div().id("voice-end").track_focus(&self.end_focus).cursor_pointer().px(px(12.0)).py(px(6.0)).rounded_full().bg(theme.text).text_color(theme.bg)
                    .on_click(move |_,_,cx|controller.update(cx,|voice,cx|voice.cancel(cx)))
                    .child(if phase==VoicePhase::Starting {"Cancel"} else {"End voice"}))
                .when(working,|el|el.child(div().id("voice-stop-task").track_focus(&self.task_focus).cursor_pointer()
                    .on_click(move |_,_,cx|composer.update(cx,|composer,cx|composer.interrupt_selected(cx)))
                    .child("Stop task"))))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn visual_state_keeps_muting_and_playback_orthogonal() {
        let mut s=VoiceSnapshot { session_id:"id".into(),chat_id:"chat".into(),generation:1,phase:VoicePhase::Active,muted:true,playing:true,work:VoiceWork::Idle,reason:None };
        assert_eq!(orb_state(s.phase,Some(&s)),OrbState::Composing);
        s.playing=false; assert_eq!(orb_state(s.phase,Some(&s)),OrbState::Breathing);
        s.muted=false; assert_eq!(orb_state(s.phase,Some(&s)),OrbState::Listening);
        assert_eq!(orb_state(VoicePhase::Starting,Some(&s)),OrbState::Connecting);
    }
}
