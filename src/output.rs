use swayipc_async::{Connection as SwayIpc, Fallible, Output};

fn display_matches(o: &Output, s: &str) -> bool {
    s == o.name
        || s == o.make
        || s == o.model
        || (o.make.len() + o.model.len() + 1 == s.len()
            && s.as_bytes()[o.make.len()] == b' '
            && s.starts_with(&o.make)
            && s.ends_with(&o.model))
        || (o.make.len() + o.model.len() + o.serial.len() + 2 == s.len()
            && s.as_bytes()[o.make.len()] == b' '
            && s.as_bytes()[o.make.len() + o.model.len() + 1] == b' '
            && s.starts_with(&o.make)
            && s.split_at(o.make.len() + 1).1.starts_with(&o.model)
            && s.ends_with(&o.serial))
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct OnOutputAdded {
    pub display: Option<String>,
    pub exec: String,
    pub args: Vec<String>,
}
impl OnOutputAdded {
    pub async fn exec(&self, o: &Output) {
        if self
            .display
            .as_ref()
            .is_some_and(|display| !display_matches(o, display))
        {
            return;
        }
        {
            use std::io::Write;
            let mut child = crate::run_cmd(&self.exec, &self.args, std::process::Stdio::piped())
                .stdin
                .take()
                .unwrap();
            child.write_all(o.name.as_bytes()).unwrap();
            child.write_all(b"\n").unwrap();
            child.flush().unwrap();
            drop(child);
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct OnOutputRemoved {
    pub display: Option<String>,
    pub exec: String,
    pub args: Vec<String>,
}
impl OnOutputRemoved {
    pub async fn exec(&self, o: &Output) {
        if self
            .display
            .as_ref()
            .is_some_and(|display| !display_matches(o, display))
        {
            return;
        }
        {
            use std::io::Write;
            let mut child = crate::run_cmd(&self.exec, &self.args, std::process::Stdio::piped())
                .stdin
                .take()
                .unwrap();
            child.write_all(o.name.as_bytes()).unwrap();
            child.write_all(b"\n").unwrap();
            child.flush().unwrap();
            drop(child);
        }
    }
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct OnOutputChanged {
    pub display: Option<String>,
    pub changes_to: Option<DeltaBits>,
    pub exec: String,
    pub args: Vec<String>,
}
impl OnOutputChanged {
    pub async fn exec(&self, o: &Output, d: &OutputDelta) {
        if self
            .display
            .as_ref()
            .is_some_and(|display| !display_matches(o, display))
        {
            return;
        }
        if self.changes_to.is_some_and(|c| (d.0 & c).is_empty()) {
            return;
        }
        {
            use std::io::Write;
            let mut child = crate::run_cmd(&self.exec, &self.args, std::process::Stdio::piped())
                .stdin
                .take()
                .unwrap();
            child.write_all(o.name.as_bytes()).unwrap();
            child.write_all(b"\n").unwrap();
            child.flush().unwrap();
            drop(child);
        }
    }
}

#[derive(Debug)]
pub enum OutputChange {
    Added(swayipc_async::Output),
    Removed(swayipc_async::Output),
    Changed(swayipc_async::Output, OutputDelta),
}
pub struct Outputs(std::collections::BTreeMap<i64, swayipc_async::Output>);
impl Outputs {
    pub async fn new(swayipc: &mut SwayIpc) -> Fallible<Self> {
        let mut me = Self(Default::default());
        for output in swayipc.get_outputs().await? {
            if let Some(id) = output.id {
                me.0.insert(id, output);
            }
        }
        Ok(me)
    }
    pub async fn update(&mut self, swayipc: &mut SwayIpc) -> Fallible<Vec<OutputChange>> {
        let mut v = vec![];
        let mut unfound: std::collections::BTreeSet<_> = self.0.keys().copied().collect();
        for output in swayipc.get_outputs().await? {
            if let Some(id) = output.id {
                use std::collections::btree_map::Entry::*;
                match self.0.entry(id) {
                    Vacant(e) => {
                        e.insert(output.clone());
                        v.push(OutputChange::Added(output))
                    }
                    Occupied(mut e) => {
                        unfound.remove(&id);
                        let former = e.get_mut();
                        let d = OutputDelta::between(former, &output);
                        if d.is_some() {
                            if d.same_specs() {
                                unfound.remove(&id);
                                v.push(OutputChange::Removed(e.insert(output.clone())));
                                v.push(OutputChange::Added(output));
                            } else {
                                *former = output.clone();
                                v.push(OutputChange::Changed(output, d));
                            }
                        }
                    }
                }
            }
        }
        for unfound in unfound {
            if let Some(r) = self.0.remove(&unfound) {
                v.push(OutputChange::Removed(r));
            }
        }
        Ok(v)
    }
}

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, Default)]
    pub struct DeltaBits: u32 {
        const id = 1;
        const name = 2;
        const make = 4;
        const model = 8;
        const serial = 0x10;
        const active = 0x20;
        const dpms = 0x40;
        const primary = 0x80;
        const scale = 0x100;
        const subpixel_hinting = 0x200;
        const transform = 0x400;
        const current_workspace = 0x800;
        const modes = 0x1000;
        const current_mode = 0x2000;
        const rect = 0x4000;
        const focus = 0x8000;
        const focused = 0x1_0000;
    }
}
impl DeltaBits {
    const SPECS: Self = Self::make.union(Self::model).union(Self::serial);
}
#[derive(Debug)]
pub struct OutputDelta(DeltaBits);
impl OutputDelta {
    fn between(o: &swayipc_async::Output, n: &swayipc_async::Output) -> Self {
        let swayipc_async::Output {
            id: o_id,
            name: o_name,
            make: o_make,
            model: o_model,
            serial: o_serial,
            active: o_active,
            dpms: o_dpms,
            primary: o_primary,
            scale: o_scale,
            subpixel_hinting: o_subpixel_hinting,
            transform: o_transform,
            current_workspace: o_current_workspace,
            modes: o_modes,
            current_mode: o_current_mode,
            rect: o_rect,
            focus: o_focus,
            focused: o_focused,
            ..
        } = o;
        let swayipc_async::Output {
            id: n_id,
            name: n_name,
            make: n_make,
            model: n_model,
            serial: n_serial,
            active: n_active,
            dpms: n_dpms,
            primary: n_primary,
            scale: n_scale,
            subpixel_hinting: n_subpixel_hinting,
            transform: n_transform,
            current_workspace: n_current_workspace,
            modes: n_modes,
            current_mode: n_current_mode,
            rect: n_rect,
            focus: n_focus,
            focused: n_focused,
            ..
        } = n;
        let mut r = DeltaBits::empty();
        if o_id != n_id {
            r |= DeltaBits::id
        }
        if o_name != n_name {
            r |= DeltaBits::name
        }
        if o_make != n_make {
            r |= DeltaBits::make
        }
        if o_model != n_model {
            r |= DeltaBits::model
        }
        if o_serial != n_serial {
            r |= DeltaBits::serial
        }
        if o_active != n_active {
            r |= DeltaBits::active
        }
        if o_dpms != n_dpms {
            r |= DeltaBits::dpms
        }
        if o_primary != n_primary {
            r |= DeltaBits::primary
        }
        if o_scale != n_scale {
            r |= DeltaBits::scale
        }
        if o_subpixel_hinting != n_subpixel_hinting {
            r |= DeltaBits::subpixel_hinting
        }
        if o_transform != n_transform {
            r |= DeltaBits::transform
        }
        if o_current_workspace != n_current_workspace {
            r |= DeltaBits::current_workspace
        }
        if o_modes != n_modes {
            r |= DeltaBits::modes
        }
        if o_current_mode != n_current_mode {
            r |= DeltaBits::current_mode
        }
        if o_rect != n_rect {
            r |= DeltaBits::rect
        }
        if o_focus != n_focus {
            r |= DeltaBits::focus
        }
        if o_focused != n_focused {
            r |= DeltaBits::focused
        }
        Self(r)
    }
    fn same_specs(&self) -> bool {
        (self.0 & DeltaBits::SPECS).is_empty()
    }
    fn is_some(&self) -> bool {
        !self.0.is_empty()
    }
}
