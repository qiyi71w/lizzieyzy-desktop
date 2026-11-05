//! 抓取死活题 (Java helper `CaptureTsumeGo1.2.jar`, `captureTsumeGo.CaptureTsumego`): three clicks on
//! a screen picture of a board mark its grid, and every intersection of the marked area is read as
//! empty, black or white from the picture's pixels.
//!
//! Coordinates are screen pixels of the picture. The steps:
//!
//! 1. the origin intersection `(0,0)`;
//! 2. the diagonal intersection of the first square: the grid spacing (a zero spacing cancels) and
//!    the corner of the board the problem sits in ([`direction`]);
//! 3. any point: the marked area runs from the origin to it, its spacing fitted to a whole number
//!    of squares ([`Selection::new`]); every move redraws it with the stones read so far.
//!
//! Recognition counts the near-gray pixels of a spacing-sized box around each intersection: black
//! when enough are dark, white when enough are light and the box is a stone rather than a flat
//! white background (its upper-left edge is not all white, or it has shading). Java's arithmetic
//! is kept: `Math.round` rounds half up, the box positions are computed in `float`.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::settings::CaptureSettings;

/// A screen picture: `width * height` pixels, 4 bytes each (R, G, B, unused), row-major.
pub struct Picture<'a> {
    pub width: u32,
    pub height: u32,
    pub rgba: &'a [u8],
}

impl Picture<'_> {
    /// The pixel at screen `(x, y)`; outside the screen it is black, as a screen capture of an
    /// area beyond the display reads.
    fn rgb(&self, x: i64, y: i64) -> (i32, i32, i32) {
        if x < 0 || y < 0 || x >= i64::from(self.width) || y >= i64::from(self.height) {
            return (0, 0, 0);
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        (
            i32::from(self.rgba[i]),
            i32::from(self.rgba[i + 1]),
            i32::from(self.rgba[i + 2]),
        )
    }
}

/// A captured problem as the helper reports it (`dx <direction> bw <w> bh <h>`, then one `#` row
/// per line): `stones` is row-major, 0 empty, 1 black, 2 white.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub struct CapturedProblem {
    /// The board corner the problem is placed in: 1 upper left, 2 upper right, 3 lower left,
    /// 4 lower right.
    pub direction: u8,
    pub width: u16,
    pub height: u16,
    pub stones: Vec<u8>,
}

/// The marked area of step 3 and the stones read in it.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
pub struct CapturePreview {
    pub x_min: i32,
    pub y_min: i32,
    pub width: i32,
    pub height: i32,
    /// The fitted spacing of the squares.
    pub cell_width: f64,
    pub cell_height: f64,
    /// Squares across and down (`cols`, `rows`); the board has one more line each way.
    pub columns: i32,
    pub rows: i32,
    /// Row-major over `(columns + 1) * (rows + 1)` intersections: 0 empty, 1 black, 2 white.
    pub stones: Vec<u8>,
}

/// Java `Math.round(double)` (half up; NaN is 0).
fn round(x: f64) -> i64 {
    if x.is_nan() {
        0
    } else {
        (x + 0.5).floor() as i64
    }
}

/// Java `Math.round(float)`.
fn round_f(x: f32) -> i64 {
    if x.is_nan() {
        0
    } else {
        (x + 0.5).floor() as i64
    }
}

/// Step 2's grid spacing from the origin and the clicked point; `None` (a zero spacing) cancels.
pub fn spacing(origin: (i32, i32), at: (i32, i32)) -> Option<(i32, i32)> {
    let gap = ((origin.0 - at.0).abs(), (origin.1 - at.1).abs());
    (gap.0 != 0 && gap.1 != 0).then_some(gap)
}

/// Step 2's corner: where the first square lies from the origin. Only called with a non-zero
/// [`spacing`], so the point is in one of the four quadrants.
pub fn direction(origin: (i32, i32), at: (i32, i32)) -> u8 {
    match (at.0 > origin.0, at.1 > origin.1) {
        (true, true) => 1,
        (false, true) => 2,
        (true, false) => 3,
        (false, false) => 4,
    }
}

/// Step 3's area from the origin to `cursor` with squares of about `gap`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
    x_min: i32,
    y_min: i32,
    width: i32,
    height: i32,
    cell_width: f64,
    cell_height: f64,
    columns: i32,
    rows: i32,
}

impl Selection {
    /// `None` while the area is narrower than half a square either way (Java divides by zero
    /// there and reads nothing).
    pub fn new(origin: (i32, i32), gap: (i32, i32), cursor: (i32, i32)) -> Option<Selection> {
        let dx = (cursor.0 - origin.0).abs();
        let dy = (cursor.1 - origin.1).abs();
        let cell_width = f64::from(dx) / round(f64::from(dx) / f64::from(gap.0)) as f64;
        let cell_height = f64::from(dy) / round(f64::from(dy) / f64::from(gap.1)) as f64;
        if !(cell_width.is_finite() && cell_height.is_finite() && cell_width > 0.0 && cell_height > 0.0) {
            return None;
        }
        let (width, height) = (dx, dy);
        Some(Selection {
            x_min: origin.0.min(cursor.0),
            y_min: origin.1.min(cursor.1),
            width,
            height,
            cell_width,
            cell_height,
            columns: round(f64::from(width) / cell_width) as i32,
            rows: round(f64::from(height) / cell_height) as i32,
        })
    }

    pub fn board_width(&self) -> i32 {
        self.columns + 1
    }

    pub fn board_height(&self) -> i32 {
        self.rows + 1
    }

    /// The stones of the area in `picture` (`recognizeBoard`), row-major.
    pub fn recognize(&self, picture: &Picture, settings: &CaptureSettings) -> Vec<u8> {
        // The captured image starts half a square before the area and is one square larger.
        let left = round(f64::from(self.x_min) - self.cell_width / 2.0);
        let top = round(f64::from(self.y_min) - self.cell_height / 2.0);
        let image_width = round(f64::from(self.width) + self.cell_width);
        let image_height = round(f64::from(self.height) + self.cell_height);
        let (gap_w, gap_h) = (self.cell_width as f32, self.cell_height as f32);
        let sample_w = round_f(gap_w);
        let sample_h = round_f(gap_h);
        let (bw, bh) = (self.board_width(), self.board_height());
        let mut stones = Vec::with_capacity((bw * bh) as usize);
        for row in 0..bh {
            for col in 0..bw {
                let mut start_x = round_f(col as f32 * gap_w);
                let mut start_y = round_f(row as f32 * gap_h);
                if start_x + sample_w > image_width {
                    start_x = image_width - sample_w;
                }
                if start_y + sample_h > image_height {
                    start_y = image_height - sample_h;
                }
                let at = |x: i64, y: i64| picture.rgb(left + start_x + x, top + start_y + y);
                stones.push(classify(at, sample_w, sample_h, settings));
            }
        }
        stones
    }

    pub fn preview(&self, picture: &Picture, settings: &CaptureSettings) -> CapturePreview {
        CapturePreview {
            x_min: self.x_min,
            y_min: self.y_min,
            width: self.width,
            height: self.height,
            cell_width: self.cell_width,
            cell_height: self.cell_height,
            columns: self.columns,
            rows: self.rows,
            stones: self.recognize(picture, settings),
        }
    }

    /// The finished capture (`close`): its size, the corner from step 2 and the stones.
    pub fn problem(&self, direction: u8, picture: &Picture, settings: &CaptureSettings) -> CapturedProblem {
        CapturedProblem {
            direction,
            width: self.board_width().clamp(0, i32::from(u16::MAX)) as u16,
            height: self.board_height().clamp(0, i32::from(u16::MAX)) as u16,
            stones: self.recognize(picture, settings),
        }
    }
}

/// `getColorPercent` + the decision of `recognizeBoard` for one box.
fn classify(at: impl Fn(i64, i64) -> (i32, i32, i32), w: i64, h: i64, s: &CaptureSettings) -> u8 {
    let total = w * h;
    if total <= 0 {
        return 0;
    }
    let white_min = 255 - s.white_offset;
    let (mut black, mut white, mut pure, mut almost) = (0i64, 0i64, 0i64, 0i64);
    for y in 0..h {
        for x in 0..w {
            let (r, g, b) = at(x, y);
            if (r - g).abs() >= s.gray_offset || (g - b).abs() >= s.gray_offset || (b - r).abs() >= s.gray_offset {
                continue;
            }
            if r <= s.black_offset && g <= s.black_offset && b <= s.black_offset {
                black += 1;
            }
            if r >= white_min && g >= white_min && b >= white_min {
                white += 1;
            }
            if r >= 225 && g >= 225 && b >= 225 {
                pure += 1;
            }
            if r >= 190 && g >= 190 && b >= 190 {
                almost += 1;
            }
        }
    }
    let percent = |n: i64| (100 * n / total) as i32;
    if percent(black) >= s.black_percent {
        return 1;
    }
    if percent(white) < s.white_percent {
        return 0;
    }
    // A stone leaves board colour around its edge: the row a quarter down is not white all
    // through its first third.
    let row = round(h as f64 / 4.0);
    let limit = round(w as f64 * 2.0 / 6.0);
    let edged = (0..limit).any(|x| {
        let (r, g, b) = at(x, row);
        r < 225 || g < 225 || b < 225
    });
    if edged || percent(almost) - percent(pure) >= 10 {
        2
    } else {
        0
    }
}
