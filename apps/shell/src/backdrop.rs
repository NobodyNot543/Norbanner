//! The frosted backdrop behind the launcher: the wallpaper, small and
//! heavily blurred, so drawing it stretched under a tint reads as glass.

use norbanner_shell::icons::IconImage;

/// Blurs `image` in place with three box passes each way (close to a
/// Gaussian of the same radius). Edges repeat their last pixel.
pub fn blur(image: &mut IconImage, radius: usize) {
    let (w, h) = (image.width as usize, image.height as usize);
    if w == 0 || h == 0 || radius == 0 || image.bgra.len() < w * h * 4 {
        return;
    }
    let mut scratch = vec![0u8; w * h * 4];
    for _ in 0..3 {
        pass(&image.bgra, &mut scratch, w, h, radius, true);
        pass(&scratch, &mut image.bgra, w, h, radius, false);
    }
}

/// One box pass along rows (`horizontal`) or columns, `src` into `dst`.
fn pass(src: &[u8], dst: &mut [u8], w: usize, h: usize, radius: usize, horizontal: bool) {
    let (lines, len) = if horizontal { (h, w) } else { (w, h) };
    let at = |line: usize, i: usize| {
        if horizontal {
            (line * w + i) * 4
        } else {
            (i * w + line) * 4
        }
    };
    let span = (radius * 2 + 1) as u32;
    for line in 0..lines {
        let mut sum = [0u32; 4];
        // The window around index 0, edges clamped.
        for k in 0..=radius * 2 {
            let i = k.saturating_sub(radius).min(len - 1);
            let p = at(line, i);
            for c in 0..4 {
                sum[c] += u32::from(src[p + c]);
            }
        }
        for i in 0..len {
            let p = at(line, i);
            for c in 0..4 {
                dst[p + c] = ((sum[c] + span / 2) / span) as u8;
            }
            let out = at(line, i.saturating_sub(radius));
            let inn = at(line, (i + radius + 1).min(len - 1));
            for c in 0..4 {
                sum[c] = sum[c] + u32::from(src[inn + c]) - u32::from(src[out + c]);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image(w: u32, h: u32, f: impl Fn(u32, u32) -> u8) -> IconImage {
        let mut bgra = Vec::new();
        for y in 0..h {
            for x in 0..w {
                let v = f(x, y);
                bgra.extend_from_slice(&[v, v, v, 255]);
            }
        }
        IconImage {
            width: w,
            height: h,
            bgra,
        }
    }

    #[test]
    fn flat_stays_flat() {
        let mut img = image(9, 7, |_, _| 120);
        blur(&mut img, 3);
        assert!(img.bgra.chunks(4).all(|p| p == [120, 120, 120, 255]));
    }

    #[test]
    fn hard_edge_softens() {
        let mut img = image(20, 4, |x, _| if x < 10 { 0 } else { 255 });
        blur(&mut img, 2);
        let at = |x: usize| img.bgra[x * 4];
        assert!(at(0) < 10 && at(19) > 245);
        assert!(at(9) > 40 && at(10) < 215);
        assert!((0..19).all(|x| at(x) <= at(x + 1)));
    }
}
