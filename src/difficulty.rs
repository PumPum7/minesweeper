#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DifficultyPreset {
    Beginner,
    Intermediate,
    Expert,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DifficultySettings {
    pub width: usize,
    pub height: usize,
    pub mines: usize,
    pub label: String,
}

impl DifficultyPreset {
    pub fn settings(self) -> DifficultySettings {
        match self {
            Self::Beginner => DifficultySettings {
                width: 9,
                height: 9,
                mines: 10,
                label: "Beginner".to_string(),
            },
            Self::Intermediate => DifficultySettings {
                width: 16,
                height: 16,
                mines: 40,
                label: "Intermediate".to_string(),
            },
            Self::Expert => DifficultySettings {
                width: 30,
                height: 16,
                mines: 99,
                label: "Expert".to_string(),
            },
        }
    }
}

pub fn validate_custom(
    width: usize,
    height: usize,
    mines: usize,
) -> Result<DifficultySettings, String> {
    const MIN_SIDE: usize = 5;
    const MAX_SIDE: usize = 50;

    if !(MIN_SIDE..=MAX_SIDE).contains(&width) {
        return Err(format!("Width must be between {MIN_SIDE} and {MAX_SIDE}."));
    }

    if !(MIN_SIDE..=MAX_SIDE).contains(&height) {
        return Err(format!("Height must be between {MIN_SIDE} and {MAX_SIDE}."));
    }

    let total = width * height;
    if mines == 0 {
        return Err("Mines must be at least 1.".to_string());
    }

    if mines >= total {
        return Err("Mines must be less than total cell count.".to_string());
    }

    Ok(DifficultySettings {
        width,
        height,
        mines,
        label: "Custom".to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_custom_bounds() {
        assert!(validate_custom(4, 5, 3).is_err());
        assert!(validate_custom(5, 4, 3).is_err());
        assert!(validate_custom(5, 5, 0).is_err());
        assert!(validate_custom(5, 5, 25).is_err());
        assert!(validate_custom(50, 50, 1).is_ok());
    }
}
