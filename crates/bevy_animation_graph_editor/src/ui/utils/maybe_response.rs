use std::ops::BitOrAssign;

#[derive(Default)]
pub struct MaybeResponse(pub Option<egui::Response>);

impl MaybeResponse {
    pub fn changed(&self) -> bool {
        self.0.as_ref().is_some_and(|inner| inner.changed())
    }

    pub fn mark_changed(&mut self) {
        if let Some(inner) = &mut self.0 {
            inner.mark_changed();
        }
    }
}

impl BitOrAssign<egui::Response> for MaybeResponse {
    fn bitor_assign(&mut self, rhs: egui::Response) {
        if let Some(inner) = &mut self.0 {
            *inner |= rhs;
        } else {
            self.0 = Some(rhs);
        }
    }
}
