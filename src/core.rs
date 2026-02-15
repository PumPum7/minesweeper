use std::collections::VecDeque;

use crate::difficulty::DifficultySettings;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameStatus {
    Ready,
    Running,
    Won,
    Lost,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CellView {
    pub revealed: bool,
    pub flagged: bool,
    pub mine: bool,
    pub adjacent: u8,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Cell {
    mine: bool,
    adjacent: u8,
    revealed: bool,
    flagged: bool,
}

pub struct Game {
    settings: DifficultySettings,
    cells: Vec<Cell>,
    status: GameStatus,
    mines_placed: bool,
    revealed_safe_cells: usize,
    flagged_cells: usize,
    started_at_ms: Option<f64>,
    finished_at_ms: Option<f64>,
}

impl Game {
    pub fn new(settings: DifficultySettings) -> Self {
        let total = settings.width * settings.height;
        Self {
            settings,
            cells: vec![Cell::default(); total],
            status: GameStatus::Ready,
            mines_placed: false,
            revealed_safe_cells: 0,
            flagged_cells: 0,
            started_at_ms: None,
            finished_at_ms: None,
        }
    }

    pub fn reset(&mut self, settings: DifficultySettings) {
        *self = Self::new(settings);
    }

    pub fn settings(&self) -> &DifficultySettings {
        &self.settings
    }

    pub fn status(&self) -> GameStatus {
        self.status
    }

    pub fn flags_left(&self) -> i32 {
        self.settings.mines as i32 - self.flagged_cells as i32
    }

    pub fn elapsed_ms(&self, now_ms: f64) -> u64 {
        match (self.started_at_ms, self.finished_at_ms) {
            (Some(start), Some(end)) => (end - start).max(0.0) as u64,
            (Some(start), None) => (now_ms - start).max(0.0) as u64,
            _ => 0,
        }
    }

    pub fn cell(&self, x: usize, y: usize) -> Option<CellView> {
        let idx = self.index(x, y)?;
        let cell = self.cells[idx];
        Some(CellView {
            revealed: cell.revealed,
            flagged: cell.flagged,
            mine: cell.mine,
            adjacent: cell.adjacent,
        })
    }

    pub fn toggle_flag(&mut self, x: usize, y: usize) -> bool {
        if matches!(self.status, GameStatus::Won | GameStatus::Lost) {
            return false;
        }

        let Some(idx) = self.index(x, y) else {
            return false;
        };

        let cell = &mut self.cells[idx];
        if cell.revealed {
            return false;
        }

        if cell.flagged {
            cell.flagged = false;
            self.flagged_cells = self.flagged_cells.saturating_sub(1);
        } else {
            cell.flagged = true;
            self.flagged_cells += 1;
        }

        true
    }

    pub fn reveal(&mut self, x: usize, y: usize, now_ms: f64) -> bool {
        if matches!(self.status, GameStatus::Won | GameStatus::Lost) {
            return false;
        }

        let Some(idx) = self.index(x, y) else {
            return false;
        };

        if self.cells[idx].flagged || self.cells[idx].revealed {
            return false;
        }

        if !self.mines_placed {
            self.place_mines(idx);
            self.mines_placed = true;
            self.started_at_ms = Some(now_ms);
            self.status = GameStatus::Running;
        }

        if self.cells[idx].mine {
            self.cells[idx].revealed = true;
            self.status = GameStatus::Lost;
            self.finished_at_ms = Some(now_ms);
            self.reveal_all_mines();
            return true;
        }

        self.reveal_flood_fill(idx);

        if self.revealed_safe_cells == self.cells.len() - self.settings.mines {
            self.status = GameStatus::Won;
            self.finished_at_ms = Some(now_ms);
            self.flag_all_mines();
        }

        true
    }

    pub fn chord_reveal(&mut self, x: usize, y: usize, now_ms: f64) -> bool {
        if matches!(self.status, GameStatus::Won | GameStatus::Lost) {
            return false;
        }

        let Some(idx) = self.index(x, y) else {
            return false;
        };

        let selected = self.cells[idx];
        if !selected.revealed || selected.mine || selected.adjacent == 0 {
            return false;
        }

        let neighbors = self.neighbor_indices(idx);
        let flagged_count = neighbors
            .iter()
            .filter(|neighbor| self.cells[**neighbor].flagged)
            .count() as u8;
        if flagged_count != selected.adjacent {
            return false;
        }

        let mut changed = false;
        for neighbor in neighbors {
            if self.cells[neighbor].revealed || self.cells[neighbor].flagged {
                continue;
            }

            changed = true;
            if self.cells[neighbor].mine {
                self.cells[neighbor].revealed = true;
                self.status = GameStatus::Lost;
                self.finished_at_ms = Some(now_ms);
                self.reveal_all_mines();
                return true;
            }

            self.reveal_flood_fill(neighbor);
        }

        if !changed {
            return false;
        }

        if self.revealed_safe_cells == self.cells.len() - self.settings.mines {
            self.status = GameStatus::Won;
            self.finished_at_ms = Some(now_ms);
            self.flag_all_mines();
        }

        true
    }

    fn reveal_flood_fill(&mut self, start_idx: usize) {
        let mut queue = VecDeque::from([start_idx]);

        while let Some(idx) = queue.pop_front() {
            if self.cells[idx].revealed || self.cells[idx].flagged {
                continue;
            }

            self.cells[idx].revealed = true;
            if !self.cells[idx].mine {
                self.revealed_safe_cells += 1;
            }

            if self.cells[idx].adjacent == 0 {
                for neighbor in self.neighbor_indices(idx) {
                    if !self.cells[neighbor].revealed && !self.cells[neighbor].flagged {
                        queue.push_back(neighbor);
                    }
                }
            }
        }
    }

    fn place_mines(&mut self, excluded_idx: usize) {
        let mut candidates: Vec<usize> = (0..self.cells.len())
            .filter(|idx| *idx != excluded_idx)
            .collect();

        for i in 0..self.settings.mines {
            let remaining = candidates.len() - i;
            let pick = i + random_usize(remaining);
            candidates.swap(i, pick);
            let mine_idx = candidates[i];
            self.cells[mine_idx].mine = true;
        }

        self.recompute_adjacency();
    }

    fn recompute_adjacency(&mut self) {
        for idx in 0..self.cells.len() {
            if self.cells[idx].mine {
                self.cells[idx].adjacent = 0;
                continue;
            }

            let mine_count = self
                .neighbor_indices(idx)
                .into_iter()
                .filter(|neighbor| self.cells[*neighbor].mine)
                .count();

            self.cells[idx].adjacent = mine_count as u8;
        }
    }

    fn reveal_all_mines(&mut self) {
        for cell in &mut self.cells {
            if cell.mine {
                cell.revealed = true;
            }
        }
    }

    fn flag_all_mines(&mut self) {
        for cell in &mut self.cells {
            if cell.mine && !cell.flagged {
                cell.flagged = true;
                self.flagged_cells += 1;
            }
        }
    }

    fn index(&self, x: usize, y: usize) -> Option<usize> {
        if x >= self.settings.width || y >= self.settings.height {
            return None;
        }

        Some(y * self.settings.width + x)
    }

    fn neighbor_indices(&self, idx: usize) -> Vec<usize> {
        let width = self.settings.width;
        let height = self.settings.height;
        let x = idx % width;
        let y = idx / width;

        let min_x = x.saturating_sub(1);
        let max_x = (x + 1).min(width - 1);
        let min_y = y.saturating_sub(1);
        let max_y = (y + 1).min(height - 1);

        let mut neighbors = Vec::with_capacity(8);
        for ny in min_y..=max_y {
            for nx in min_x..=max_x {
                if nx == x && ny == y {
                    continue;
                }

                neighbors.push(ny * width + nx);
            }
        }

        neighbors
    }
}

#[cfg(target_arch = "wasm32")]
fn random_usize(max_exclusive: usize) -> usize {
    debug_assert!(max_exclusive > 0);
    (js_sys::Math::random() * max_exclusive as f64).floor() as usize
}

#[cfg(not(target_arch = "wasm32"))]
fn random_usize(max_exclusive: usize) -> usize {
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEED: AtomicU64 = AtomicU64::new(0x517c_c1b7_2722_0a95);

    debug_assert!(max_exclusive > 0);

    let mut value = SEED.load(Ordering::Relaxed);
    value ^= value << 7;
    value ^= value >> 9;
    value ^= value << 8;
    SEED.store(value, Ordering::Relaxed);

    (value as usize) % max_exclusive
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom(width: usize, height: usize, mines: usize) -> DifficultySettings {
        DifficultySettings {
            width,
            height,
            mines,
            label: "Test".to_string(),
        }
    }

    #[test]
    fn first_click_is_always_safe() {
        let mut game = Game::new(custom(9, 9, 10));

        game.reveal(4, 4, 100.0);

        let clicked = game.cell(4, 4).expect("cell should exist");
        assert!(!clicked.mine);

        let mine_count = game.cells.iter().filter(|cell| cell.mine).count();
        assert_eq!(mine_count, 10);
        assert_eq!(game.status(), GameStatus::Running);
    }

    #[test]
    fn flood_fill_reveals_empty_region() {
        let mut game = Game::new(custom(3, 3, 1));
        game.mines_placed = true;
        game.status = GameStatus::Running;
        game.started_at_ms = Some(0.0);

        game.cells[8].mine = true;
        game.recompute_adjacency();

        game.reveal(0, 0, 10.0);

        let revealed_count = game.cells.iter().filter(|cell| cell.revealed).count();
        assert_eq!(revealed_count, 8);
        assert!(game.cells[8].flagged);
        assert_eq!(game.status(), GameStatus::Won);
    }

    #[test]
    fn toggle_flag_blocks_reveal() {
        let mut game = Game::new(custom(5, 5, 3));

        assert!(game.toggle_flag(1, 1));
        assert!(game.cell(1, 1).expect("cell should exist").flagged);
        assert!(!game.reveal(1, 1, 10.0));
    }

    #[test]
    fn chord_reveals_neighbors_when_flags_match() {
        let mut game = Game::new(custom(3, 3, 1));
        game.mines_placed = true;
        game.status = GameStatus::Running;
        game.started_at_ms = Some(0.0);

        game.cells[0].mine = true;
        game.recompute_adjacency();

        game.cells[4].revealed = true;
        game.revealed_safe_cells = 1;
        game.cells[0].flagged = true;
        game.flagged_cells = 1;

        assert!(game.chord_reveal(1, 1, 15.0));
        assert_eq!(game.status(), GameStatus::Won);
        assert!(game.cells.iter().all(|cell| cell.revealed || (cell.mine && cell.flagged)));
    }

    #[test]
    fn chord_does_nothing_when_flag_count_mismatch() {
        let mut game = Game::new(custom(3, 3, 1));
        game.mines_placed = true;
        game.status = GameStatus::Running;
        game.started_at_ms = Some(0.0);

        game.cells[0].mine = true;
        game.recompute_adjacency();

        game.cells[4].revealed = true;
        game.revealed_safe_cells = 1;

        assert!(!game.chord_reveal(1, 1, 15.0));
        assert!(!game.cells[1].revealed);
        assert_eq!(game.status(), GameStatus::Running);
    }
}
