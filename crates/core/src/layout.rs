use num_traits::ToPrimitive;
use serde::Deserialize;

/// Total gap between two neighbouring keys, in pixels. Each key is inset by
/// half of it on every side, and touches in the gap hit no key.
pub const KEY_GAP_PX: u16 = 6;

const KEY_INSET_PX: u16 = KEY_GAP_PX / 2;

/// Screen size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub w: u16,
    pub h: u16,
}

/// Axis-aligned rectangle in screen pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl Rect {
    /// Whether the point lies inside the rectangle (right and bottom edges
    /// excluded).
    #[must_use]
    pub fn contains(self, x: u16, y: u16) -> bool {
        let right = u32::from(self.x) + u32::from(self.w);
        let bottom = u32::from(self.y) + u32::from(self.h);
        x >= self.x && y >= self.y && u32::from(x) < right && u32::from(y) < bottom
    }
}

/// Key grid of the device. Keys are numbered row by row from the top left,
/// starting at 0.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub enum Layout {
    /// Stream Deck MK.2 / 15-key: 3 rows of 5 keys.
    #[default]
    #[serde(rename = "3x5")]
    Grid3x5,
    /// 480x480 touch panel: 4 rows of 4 keys.
    #[serde(rename = "4x4")]
    Grid4x4,
}

impl Layout {
    /// Number of key rows.
    #[must_use]
    pub const fn rows(self) -> u8 {
        match self {
            Self::Grid3x5 => 3,
            Self::Grid4x4 => 4,
        }
    }

    /// Number of key columns.
    #[must_use]
    pub const fn cols(self) -> u8 {
        match self {
            Self::Grid3x5 => 5,
            Self::Grid4x4 => 4,
        }
    }

    /// Number of keys; valid key indices are `0..key_count()`.
    #[must_use]
    pub const fn key_count(self) -> u8 {
        self.rows() * self.cols()
    }

    /// Side length of one square grid cell (key plus its share of the gap).
    fn cell(self, screen: Size) -> u16 {
        screen.w / u16::from(self.cols())
    }

    /// On-screen rectangle of a key, or `None` when the index is out of range
    /// or the grid does not fit the screen.
    ///
    /// Cells are squares `screen.w / cols` wide, laid out from the top left;
    /// each key is inset by `KEY_GAP_PX / 2` on every side.
    #[must_use]
    pub fn key_rect(self, index: u8, screen: Size) -> Option<Rect> {
        if index >= self.key_count() {
            return None;
        }
        let cell = self.cell(screen);
        if cell <= KEY_GAP_PX {
            return None;
        }
        let grid_h = cell.checked_mul(u16::from(self.rows()))?;
        if grid_h > screen.h {
            return None;
        }
        let row = u16::from(index / self.cols());
        let col = u16::from(index % self.cols());
        let side = cell - KEY_GAP_PX;
        Some(Rect {
            x: col * cell + KEY_INSET_PX,
            y: row * cell + KEY_INSET_PX,
            w: side,
            h: side,
        })
    }

    /// Key under a screen point, or `None` for a point in a gap between keys
    /// or outside the grid.
    #[must_use]
    pub fn key_at(self, x: u16, y: u16, screen: Size) -> Option<u8> {
        if x >= screen.w || y >= screen.h {
            return None;
        }
        let cell = self.cell(screen);
        if cell == 0 {
            return None;
        }
        let col = (x / cell).to_u8()?;
        let row = (y / cell).to_u8()?;
        if col >= self.cols() || row >= self.rows() {
            return None;
        }
        let index = row * self.cols() + col;
        self.key_rect(index, screen)
            .filter(|rect| rect.contains(x, y))
            .map(|_| index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCREEN: Size = Size { w: 480, h: 480 };

    #[test]
    fn key_at_4x4_480() {
        let grid = Layout::Grid4x4;
        assert_eq!(grid.key_at(60, 60, SCREEN), Some(0));
        assert_eq!(grid.key_at(420, 420, SCREEN), Some(15));
        assert_eq!(grid.key_at(179, 60, SCREEN), Some(1));
        assert_eq!(grid.key_at(120, 60, SCREEN), None, "gap between keys");
        assert_eq!(grid.key_at(480, 10, SCREEN), None, "off-screen");
    }

    #[test]
    fn key_rect_4x4_480_index_5() {
        assert_eq!(
            Layout::Grid4x4.key_rect(5, SCREEN),
            Some(Rect {
                x: 123,
                y: 123,
                w: 114,
                h: 114
            })
        );
    }

    #[test]
    fn key_counts() {
        assert_eq!(Layout::Grid3x5.key_count(), 15);
        assert_eq!(Layout::Grid4x4.key_count(), 16);
        assert_eq!(Layout::default(), Layout::Grid3x5);
    }

    #[test]
    fn key_rect_out_of_range_is_none() {
        assert_eq!(Layout::Grid4x4.key_rect(16, SCREEN), None);
        assert_eq!(Layout::Grid3x5.key_rect(15, SCREEN), None);
    }

    #[test]
    fn key_rect_none_when_grid_does_not_fit() {
        // 4 rows of 120 px cells need 480 px of height.
        assert_eq!(Layout::Grid4x4.key_rect(0, Size { w: 480, h: 479 }), None);
        // Cells no wider than the gap leave no key.
        assert_eq!(Layout::Grid4x4.key_rect(0, Size { w: 24, h: 24 }), None);
    }

    #[test]
    fn every_key_rect_edge_maps_back_to_its_key() {
        for grid in [Layout::Grid3x5, Layout::Grid4x4] {
            for index in 0..grid.key_count() {
                let r = grid.key_rect(index, SCREEN).expect("key fits");
                let (right, bottom) = (r.x + r.w - 1, r.y + r.h - 1);
                for (x, y) in [(r.x, r.y), (right, bottom), (r.x, bottom), (right, r.y)] {
                    assert_eq!(grid.key_at(x, y, SCREEN), Some(index), "{grid:?} ({x},{y})");
                }
                assert_eq!(grid.key_at(r.x - 1, r.y, SCREEN), None, "left gap");
                assert_eq!(grid.key_at(right + 1, r.y, SCREEN), None, "right gap");
            }
        }
    }

    #[test]
    fn key_at_below_3x5_grid_is_none() {
        // 3 rows of 96 px cells end at y = 288 on a square screen.
        assert_eq!(Layout::Grid3x5.key_at(50, 300, SCREEN), None);
    }
}
