pub struct Point {
    x: f64,
    y: f64,
}

pub fn calculate_distance(p1: &Point, p2: &Point) -> f64 {
    ((p1.x - p2.x).powi(2) + (p1.y - p2.y).powi(2)).sqrt()
}

pub enum Color {
    Red,
    Green,
    Blue,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}
