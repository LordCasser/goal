//! Validated client geometry for the Windows caption hit targets.
use serde::Deserialize;

pub const STALE_LAYOUT: &str = "stale desktop layout";
pub const CLIENT_SIZE_CHANGED: &str = "desktop client size changed before layout arrived";

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    pub width: f64,
    pub height: f64,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Regions {
    pub revision: u64,
    pub viewport: Viewport,
    pub maximize: Rect,
    pub drag: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhysicalRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    fn valid(self, viewport: Viewport) -> bool {
        [self.x, self.y, self.width, self.height]
            .iter()
            .all(|n| n.is_finite())
            && self.x >= 0.0
            && self.y >= 0.0
            && self.width > 0.0
            && self.height > 0.0
            && self.x + self.width <= viewport.width + 0.5
            && self.y + self.height <= viewport.height + 0.5
    }

    fn overlaps(self, other: Rect) -> bool {
        self.x < other.x + other.width
            && other.x < self.x + self.width
            && self.y < other.y + other.height
            && other.y < self.y + self.height
    }

    fn physical(self, scale: f64) -> PhysicalRect {
        let x = (self.x * scale).round() as i32;
        let y = (self.y * scale).round() as i32;
        PhysicalRect {
            x,
            y,
            width: ((self.x + self.width) * scale).round() as i32 - x,
            height: ((self.y + self.height) * scale).round() as i32 - y,
        }
    }
}

impl Regions {
    pub fn validate(
        &self,
        previous: u64,
        client_width: i32,
        client_height: i32,
    ) -> Result<(PhysicalRect, PhysicalRect), String> {
        if self.revision <= previous {
            return Err(STALE_LAYOUT.into());
        }
        let v = self.viewport;
        if !v.width.is_finite()
            || !v.height.is_finite()
            || v.width < 320.0
            || v.height < 200.0
            || !self.maximize.valid(v)
            || !self.drag.valid(v)
            || self.maximize.overlaps(self.drag)
        {
            return Err("invalid desktop layout bounds".into());
        }
        // Limit native interception to the caption band and its right-hand control.
        let max = self.maximize;
        let drag = self.drag;
        let right_gap = v.width - max.x - max.width;
        if !(32.0..=64.0).contains(&max.width)
            || !(24.0..=64.0).contains(&max.height)
            || max.y > 8.0
            || !(32.0..=72.0).contains(&right_gap)
            || drag.y > 8.0
            || drag.height > 64.0
            || drag.width < 32.0
            || drag.x + drag.width > max.x
        {
            return Err("desktop layout is outside the caption band".into());
        }
        // The measured viewport includes WebView zoom. Multiplying by OS DPI a
        // second time would move the hit target away from its visible button.
        let scale = f64::from(client_width) / v.width;
        let vertical = f64::from(client_height) / v.height;
        if client_width <= 0
            || client_height <= 0
            || !(0.5..=8.0).contains(&scale)
            || (scale - vertical).abs() > 0.03 * scale
        {
            return Err(CLIENT_SIZE_CHANGED.into());
        }
        Ok((max.physical(scale), drag.physical(scale)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn regions() -> Regions {
        Regions {
            revision: 1,
            viewport: Viewport {
                width: 1280.0,
                height: 800.0,
            },
            maximize: Rect {
                x: 1188.0,
                y: 0.0,
                width: 46.0,
                height: 44.0,
            },
            drag: Rect {
                x: 100.0,
                y: 0.0,
                width: 240.0,
                height: 44.0,
            },
        }
    }
    #[test]
    fn maps_current_viewport_once_at_fractional_dpi() {
        let (max, _) = regions().validate(0, 1600, 1000).unwrap();
        assert_eq!(
            max,
            PhysicalRect {
                x: 1485,
                y: 0,
                width: 58,
                height: 55
            }
        );
        let (max, _) = regions().validate(0, 2560, 1600).unwrap();
        assert_eq!(max.width, 92);
    }
    #[test]
    fn rejects_stale_or_wrong_sized_measurements() {
        assert_eq!(regions().validate(1, 1280, 800).unwrap_err(), STALE_LAYOUT);
        assert_eq!(
            regions().validate(0, 1920, 800).unwrap_err(),
            CLIENT_SIZE_CHANGED
        );
        assert_eq!(
            regions().validate(0, 0, 0).unwrap_err(),
            CLIENT_SIZE_CHANGED
        );
    }
    #[test]
    fn rejects_nonfinite_outside_and_overlapping_regions() {
        for value in [f64::NAN, f64::INFINITY, -1.0, 1281.0] {
            let mut r = regions();
            r.maximize.x = value;
            assert!(r.validate(0, 1280, 800).is_err());
        }
        let mut r = regions();
        r.drag = r.maximize;
        assert!(r.validate(0, 1280, 800).is_err());
        let mut r = regions();
        r.maximize.y = 120.0;
        assert!(r.validate(0, 1280, 800).is_err());
    }
}
