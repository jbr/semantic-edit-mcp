// Test file for insert after struct
pub struct Point {
    x: f64,
    y: f64,
}
impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    pub fn distance_to(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

pub struct Circle {
    center: Point,
    radius: f64,
}

pub fn calculate_area(circle: &Circle) -> f64 {
    std::f64::consts::PI * circle.radius * circle.radius
}
