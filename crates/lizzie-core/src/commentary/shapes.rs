//! Names the shape a move makes on the frozen board (a hane, a cut, a 3-3 invasion, ...), so the
//! commentary can say what a move does instead of reading out coordinates. Ports Java
//! `teacher/MoveShapes.java`; only shapes that the stones alone settle are named, and any other
//! move gets no name rather than a guess.
use crate::record::{Point, Position, Stone};
use std::collections::VecDeque;

const CORNER_BOX: i32 = 5;
const SIDES: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
const DIAGONALS: [(i32, i32); 4] = [(1, 1), (1, -1), (-1, 1), (-1, -1)];

/// The shape of `r#move` played by Black or White on `stones`, if it has a name.
pub fn describe(stones: &Position, r#move: &str, black: bool) -> Option<String> {
    let point = super::evidence::parse_coordinate(r#move, stones.size)?;
    if stones.stones.len() != stones.size.area() {
        return None;
    }
    let grid = Grid::new(stones);
    if !matches!(grid.at(point), Cell::Empty) {
        return None;
    }
    let own = if black { Stone::Black } else { Stone::White };
    let opponent = own.opposite();
    // None: not tactical, look further; Some(""): a tactical move without a name.
    let named = match tactical(&grid, point, own, opponent) {
        Some(named) => named,
        None => {
            let touches = !grid.neighbors(point, own).is_empty() || !grid.neighbors(point, opponent).is_empty();
            if touches {
                contact(&grid, point, own, opponent)
            } else {
                distant(&grid, point, own, opponent)
            }
        }
    };
    named.filter(|text| !text.is_empty())
}

fn tactical(grid: &Grid, point: Point, own: Stone, opponent: Stone) -> Option<Option<String>> {
    let opponent_groups = grid.adjacent_groups(point, opponent);
    let after = grid.with(point, own);
    let mut captured = Vec::new();
    for group in &opponent_groups {
        if after.liberties(group) == 0 {
            captured.extend(group);
        }
    }
    if !captured.is_empty() {
        return Some(Some(name(
            "capture",
            "提子",
            &format!("of {}", grid.stones(&captured, opponent)),
        )));
    }
    for group in &opponent_groups {
        if after.liberties(group) == 1 {
            return Some(Some(name(
                "atari",
                "打吃",
                &format!("on {}", grid.stones(group, opponent)),
            )));
        }
    }
    let own_groups = grid.adjacent_groups(point, own);
    if own_groups.len() >= 2 {
        return Some(Some(name(
            "connection",
            "粘",
            &format!("joining {}", grid.first_stones(&own_groups, own)),
        )));
    }
    if opponent_groups.len() >= 2 {
        // Between two separate stones: a wedge across a gap, a cut across a diagonal held by our
        // own stone. Other moves between groups get no name.
        let touched = grid.neighbors(point, opponent);
        for first in &touched {
            for second in &touched {
                if grid.same_group(*first, *second) {
                    continue;
                }
                let (fx, fy) = (i32::from(first.x), i32::from(first.y));
                let (sx, sy) = (i32::from(second.x), i32::from(second.y));
                let (px, py) = (i32::from(point.x), i32::from(point.y));
                if fx + sx == 2 * px && fy + sy == 2 * py {
                    return Some(Some(name(
                        "wedge",
                        "挖",
                        &format!("between {}", grid.pair(*first, *second, opponent)),
                    )));
                }
                let across_x = fx + sx - px;
                let across_y = fy + sy - py;
                if fx != sx && fy != sy && matches!(grid.at_xy(across_x, across_y), Cell::Stone(c) if c == own) {
                    return Some(Some(name(
                        "cut",
                        "断",
                        &format!("between {}", grid.pair(*first, *second, opponent)),
                    )));
                }
            }
        }
        return Some(None);
    }
    None
}

fn contact(grid: &Grid, point: Point, own: Stone, opponent: Stone) -> Option<String> {
    let opponents = grid.neighbors(point, opponent);
    let owns = grid.neighbors(point, own);
    if opponents.is_empty() {
        for stone in &owns {
            for touched in grid.neighbors(*stone, opponent) {
                let (ax, ay) = (
                    i32::from(point.x) - i32::from(stone.x),
                    i32::from(point.y) - i32::from(stone.y),
                );
                let (bx, by) = (
                    i32::from(touched.x) - i32::from(stone.x),
                    i32::from(touched.y) - i32::from(stone.y),
                );
                // Alongside the stone it touches; straight back from it is a retreat.
                if ax * bx + ay * by == 0 {
                    return Some(name("extension", "长", &format!("from {}", grid.stone(*stone, own))));
                }
            }
        }
        return None;
    }
    // A move touching stones of both colours blocks or pushes; those are not told apart here.
    if !owns.is_empty() {
        return None;
    }
    for touched in &opponents {
        for (dx, dy) in DIAGONALS {
            let x = i32::from(point.x) + dx;
            let y = i32::from(point.y) + dy;
            if !matches!(grid.at_xy(x, y), Cell::Stone(c) if c == own) {
                continue;
            }
            let stone = Point {
                x: x as u16,
                y: y as u16,
            };
            if manhattan(stone, *touched) == 1 {
                return Some(name(
                    "hane",
                    "扳",
                    &format!("against {}", grid.stone(*touched, opponent)),
                ));
            }
        }
    }
    Some(name(
        "attachment",
        "靠",
        &format!("to {}", grid.stone(opponents[0], opponent)),
    ))
}

fn distant(grid: &Grid, point: Point, own: Stone, opponent: Stone) -> Option<String> {
    if grid.width.min(grid.height) >= 13 {
        if let Some(named) = corner(grid, point, own, opponent) {
            return Some(named);
        }
        if let Some(named) = side_extension(grid, point, own) {
            return Some(named);
        }
    }
    if let Some(named) = relation(grid, point, own) {
        return Some(named);
    }
    invasion(grid, point, own, opponent)
}

fn corner(grid: &Grid, point: Point, own: Stone, opponent: Stone) -> Option<String> {
    let px = i32::from(point.x);
    let py = i32::from(point.y);
    let corner_x = if px <= CORNER_BOX {
        0
    } else if px >= grid.width - 1 - CORNER_BOX {
        1
    } else {
        return None;
    };
    let corner_y = if py <= CORNER_BOX {
        0
    } else if py >= grid.height - 1 - CORNER_BOX {
        1
    } else {
        return None;
    };
    let mut owns = Vec::new();
    let mut opponents = Vec::new();
    for x in 0..grid.width {
        for y in 0..grid.height {
            if grid.edge_x(x, corner_x) > CORNER_BOX || grid.edge_y(y, corner_y) > CORNER_BOX {
                continue;
            }
            match grid.at_xy(x, y) {
                Cell::Stone(c) if c == own => owns.push(Point {
                    x: x as u16,
                    y: y as u16,
                }),
                Cell::Stone(c) if c == opponent => opponents.push(Point {
                    x: x as u16,
                    y: y as u16,
                }),
                _ => {}
            }
        }
    }
    let x = grid.edge_x(px, corner_x);
    let y = grid.edge_y(py, corner_y);
    if owns.is_empty() && opponents.is_empty() {
        return (between(x, 2, 4) && between(y, 2, 4)).then(|| name("move in an empty corner", "占角", ""));
    }
    if owns.is_empty() && opponents.len() == 1 {
        let stone = opponents[0];
        let stone_x = grid.edge_x(i32::from(stone.x), corner_x);
        let stone_y = grid.edge_y(i32::from(stone.y), corner_y);
        if x == 2 && y == 2 && stone_x == 3 && stone_y == 3 {
            return Some(name(
                "3-3 invasion",
                "点三三",
                &format!("under {}", grid.stone(stone, opponent)),
            ));
        }
        if between(stone_x, 2, 4) && between(stone_y, 2, 4) && x >= 2 && y >= 2 && distance(point, stone) >= 2 {
            return Some(name(
                "corner approach",
                "挂角",
                &format!("to {}", grid.stone(stone, opponent)),
            ));
        }
    }
    if opponents.is_empty() && owns.len() == 1 {
        let stone = owns[0];
        if between(grid.edge_x(i32::from(stone.x), corner_x), 2, 4)
            && between(grid.edge_y(i32::from(stone.y), corner_y), 2, 4)
            && x >= 2
            && y >= 2
            && distance(point, stone) <= 3
        {
            let relation = relation(grid, point, own);
            return Some(name(
                "corner enclosure",
                "守角",
                &match relation {
                    Some(relation) => format!("as a {relation}"),
                    None => format!("with {}", grid.stone(stone, own)),
                },
            ));
        }
    }
    None
}

fn side_extension(grid: &Grid, point: Point, own: Stone) -> Option<String> {
    for along in [[0, 1], [1, 0]] {
        // The side runs along `along`; the edge is across it.
        let edge = if along[0] == 0 {
            i32::from(point.x).min(grid.width - 1 - i32::from(point.x))
        } else {
            i32::from(point.y).min(grid.height - 1 - i32::from(point.y))
        };
        if !(2..=3).contains(&edge) {
            continue;
        }
        for direction in [1, -1] {
            for gap in 1..=5 {
                let x = i32::from(point.x) + along[0] * direction * gap;
                let y = i32::from(point.y) + along[1] * direction * gap;
                match grid.at_xy(x, y) {
                    Cell::Off => break,
                    Cell::Stone(c) if c != own => break,
                    Cell::Stone(_) => {
                        if gap < 2 {
                            break;
                        }
                        let chinese = format!("拆{}", ["一", "二", "三", "四"][(gap - 2) as usize]);
                        return Some(name(
                            "extension along the side",
                            &chinese,
                            &format!("from {}", grid.stone_xy(x, y, own)),
                        ));
                    }
                    Cell::Empty => {}
                }
            }
        }
    }
    None
}

fn relation(grid: &Grid, point: Point, own: Stone) -> Option<String> {
    let shapes = [
        (1, 1, "diagonal move", "尖"),
        (2, 0, "one-space jump", "跳"),
        (1, 2, "knight's move", "小飞"),
        (3, 0, "two-space jump", "大跳"),
        (1, 3, "large knight's move", "大飞"),
    ];
    for (a, b, english, chinese) in shapes {
        for (dx, dy) in [(a, b), (-a, b), (a, -b), (-a, -b), (b, a), (-b, a), (b, -a), (-b, -a)] {
            let x = i32::from(point.x) + dx;
            let y = i32::from(point.y) + dy;
            if matches!(grid.at_xy(x, y), Cell::Stone(c) if c == own) && grid.open_between(point, x, y) {
                return Some(name(english, chinese, &format!("from {}", grid.stone_xy(x, y, own))));
            }
        }
    }
    None
}

fn invasion(grid: &Grid, point: Point, own: Stone, opponent: Stone) -> Option<String> {
    let px = i32::from(point.x);
    let py = i32::from(point.y);
    if px.min(grid.width - 1 - px).min(py).min(grid.height - 1 - py) < 2 {
        return None;
    }
    let mut owns = 0;
    let mut opponents = 0;
    for x in px - 4..=px + 4 {
        for y in py - 4..=py + 4 {
            match grid.at_xy(x, y) {
                Cell::Stone(c) if c == own => owns += 1,
                Cell::Stone(c) if c == opponent => opponents += 1,
                _ => {}
            }
        }
    }
    (owns == 0 && opponents >= 3).then(|| name("invasion", "打入", ""))
}

fn name(english: &str, chinese: &str, detail: &str) -> String {
    if detail.is_empty() {
        format!("{english} ({chinese})")
    } else {
        format!("{english} ({chinese}) {detail}")
    }
}

fn between(value: i32, low: i32, high: i32) -> bool {
    (low..=high).contains(&value)
}

fn distance(a: Point, b: Point) -> i32 {
    (i32::from(a.x) - i32::from(b.x))
        .abs()
        .max((i32::from(a.y) - i32::from(b.y)).abs())
}

fn manhattan(a: Point, b: Point) -> i32 {
    (i32::from(a.x) - i32::from(b.x)).abs() + (i32::from(a.y) - i32::from(b.y)).abs()
}

/// One intersection: off the board, empty, or a stone.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Cell {
    Off,
    Empty,
    Stone(Stone),
}

/// The frozen stones, indexed like Java's grid: x * height + y, y from the top.
struct Grid {
    cells: Vec<Option<Stone>>,
    width: i32,
    height: i32,
}

impl Grid {
    fn new(position: &Position) -> Grid {
        let width = i32::from(position.size.width);
        let height = i32::from(position.size.height);
        let mut cells = vec![None; (width * height) as usize];
        for x in 0..width {
            for y in 0..height {
                cells[(x * height + y) as usize] = position.at(Point {
                    x: x as u16,
                    y: y as u16,
                });
            }
        }
        Grid { cells, width, height }
    }

    fn index(&self, point: Point) -> usize {
        (i32::from(point.x) * self.height + i32::from(point.y)) as usize
    }

    fn at(&self, point: Point) -> Cell {
        self.at_xy(i32::from(point.x), i32::from(point.y))
    }

    fn at_xy(&self, x: i32, y: i32) -> Cell {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return Cell::Off;
        }
        match self.cells[(x * self.height + y) as usize] {
            Some(color) => Cell::Stone(color),
            None => Cell::Empty,
        }
    }

    fn with(&self, point: Point, stone: Stone) -> Grid {
        let mut cells = self.cells.clone();
        cells[self.index(point)] = Some(stone);
        Grid {
            cells,
            width: self.width,
            height: self.height,
        }
    }

    fn edge_x(&self, x: i32, corner: i32) -> i32 {
        if corner == 0 {
            x
        } else {
            self.width - 1 - x
        }
    }

    fn edge_y(&self, y: i32, corner: i32) -> i32 {
        if corner == 0 {
            y
        } else {
            self.height - 1 - y
        }
    }

    fn neighbors(&self, point: Point, color: Stone) -> Vec<Point> {
        let mut found = Vec::new();
        for (dx, dy) in SIDES {
            let x = i32::from(point.x) + dx;
            let y = i32::from(point.y) + dy;
            if matches!(self.at_xy(x, y), Cell::Stone(c) if c == color) {
                found.push(Point {
                    x: x as u16,
                    y: y as u16,
                });
            }
        }
        found
    }

    fn adjacent_groups(&self, point: Point, color: Stone) -> Vec<Vec<usize>> {
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for neighbor in self.neighbors(point, color) {
            let index = self.index(neighbor);
            if groups.iter().all(|group| !group.contains(&index)) {
                groups.push(self.group(index));
            }
        }
        groups
    }

    fn group(&self, origin: usize) -> Vec<usize> {
        let color = self.cells[origin].expect("group origin is a stone");
        let mut group = Vec::new();
        let mut queue = VecDeque::from([origin]);
        while let Some(index) = queue.pop_front() {
            if group.contains(&index) {
                continue;
            }
            group.push(index);
            for (dx, dy) in SIDES {
                let x = index as i32 / self.height + dx;
                let y = index as i32 % self.height + dy;
                if matches!(self.at_xy(x, y), Cell::Stone(c) if c == color) {
                    queue.push_back((x * self.height + y) as usize);
                }
            }
        }
        group
    }

    fn liberties(&self, group: &[usize]) -> usize {
        let mut liberties: Vec<usize> = Vec::new();
        for &index in group {
            for (dx, dy) in SIDES {
                let x = index as i32 / self.height + dx;
                let y = index as i32 % self.height + dy;
                if matches!(self.at_xy(x, y), Cell::Empty) {
                    let liberty = (x * self.height + y) as usize;
                    if !liberties.contains(&liberty) {
                        liberties.push(liberty);
                    }
                }
            }
        }
        liberties.len()
    }

    /// Whether the points strictly between a straight jump's ends are empty.
    fn open_between(&self, point: Point, x: i32, y: i32) -> bool {
        let dx = x - i32::from(point.x);
        let dy = y - i32::from(point.y);
        if dx != 0 && dy != 0 {
            return true;
        }
        let steps = dx.abs().max(dy.abs());
        for step in 1..steps {
            let between = self.at_xy(
                i32::from(point.x) + dx.signum() * step,
                i32::from(point.y) + dy.signum() * step,
            );
            if !matches!(between, Cell::Empty) {
                return false;
            }
        }
        true
    }

    fn stone(&self, point: Point, color: Stone) -> String {
        self.stone_xy(i32::from(point.x), i32::from(point.y), color)
    }

    fn stone_xy(&self, x: i32, y: i32, color: Stone) -> String {
        format!(
            "{} {}",
            if color == Stone::Black { "B" } else { "W" },
            coordinate_xy(x, y, self.height)
        )
    }

    /// Up to three of the stones, left to right and then top to bottom.
    fn stones(&self, indices: &[usize], color: Stone) -> String {
        let mut sorted = indices.to_vec();
        sorted.sort_unstable();
        let names = sorted
            .into_iter()
            .take(3)
            .map(|index| coordinate_xy(index as i32 / self.height, index as i32 % self.height, self.height))
            .collect::<Vec<_>>()
            .join(" ");
        format!("{} {names}", if color == Stone::Black { "B" } else { "W" })
    }

    fn same_group(&self, first: Point, second: Point) -> bool {
        self.group(self.index(first)).contains(&self.index(second))
    }

    fn pair(&self, first: Point, second: Point, color: Stone) -> String {
        let a = self.index(first);
        let b = self.index(second);
        format!(
            "{} {} and {}",
            if color == Stone::Black { "B" } else { "W" },
            coordinate_xy(
                a.min(b) as i32 / self.height,
                a.min(b) as i32 % self.height,
                self.height
            ),
            coordinate_xy(
                a.max(b) as i32 / self.height,
                a.max(b) as i32 % self.height,
                self.height
            )
        )
    }

    fn first_stones(&self, groups: &[Vec<usize>], color: Stone) -> String {
        let first = groups.iter().map(|group| group[0]).collect::<Vec<_>>();
        self.stones(&first, color)
    }
}

fn coordinate_xy(x: i32, y: i32, height: i32) -> String {
    super::evidence::coordinate(
        Point {
            x: x as u16,
            y: y as u16,
        },
        height as u16,
    )
}
