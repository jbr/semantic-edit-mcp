// Test file for insert after struct
pub struct Point {
    x: f64,
    y: f64,
}

pub struct Circle {
    center: Point,
    radius: f64,
}

pub fn calculate_area(circle: &Circle) -> f64 {
    std::f64::consts::PI * circle.radius * circle.radius
}
