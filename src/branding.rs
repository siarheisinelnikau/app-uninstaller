//! Original small geometric window mark, authored directly as pixels in code.
pub fn window_icon() -> eframe::egui::IconData {
    const SIDE: usize = 32;
    let mut rgba = vec![0; SIDE * SIDE * 4];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let color = if (5..27).contains(&x) && (5..27).contains(&y) {
                if (10..22).contains(&x) && (14..18).contains(&y) {
                    [255, 235, 195, 255]
                } else if x >= 23 && y >= 23 {
                    [229, 148, 66, 255]
                } else {
                    [13, 103, 98, 255]
                }
            } else {
                [0, 0, 0, 0]
            };
            rgba[(y * SIDE + x) * 4..(y * SIDE + x + 1) * 4].copy_from_slice(&color);
        }
    }
    eframe::egui::IconData {
        rgba,
        width: SIDE as u32,
        height: SIDE as u32,
    }
}
