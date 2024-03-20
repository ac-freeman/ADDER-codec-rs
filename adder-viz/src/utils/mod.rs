use eframe::epaint::ColorImage;
use std::collections::VecDeque;
use std::error::Error;
use video_rs_adder_dep::Frame;

pub(crate) mod slider;

// pub(crate) struct PlotY {
//     pub points: VecDeque<Option<f64>>,
// }
//
// impl PlotY {
//     pub(crate) fn get_plotline(&self, name: &str, log_base: bool) -> Line {
//         let plot_points: PlotPoints = (0..1000)
//             .map(|i| {
//                 let x = i as f64;
//                 let y = self.points[i].unwrap_or(0.0);
//                 if log_base && y > 0.0 {
//                     [x, y.log10()]
//                 } else {
//                     [x, y]
//                 }
//             })
//             .collect();
//         Line::new(plot_points).name(name)
//     }
//
//     pub(crate) fn update(&mut self, new_opt: Option<f64>) {
//         match new_opt {
//             Some(new) => {
//                 if new.is_finite() {
//                     self.points.push_back(Some(new));
//                 } else {
//                     self.points.push_back(Some(0.0));
//                 }
//             }
//             None => self.points.push_back(None),
//         }
//         self.points.pop_front();
//     }
// }

pub fn prep_epaint_image(
    image_mat: &Frame,
    color: bool,
    width: usize,
    height: usize,
) -> Result<ColorImage, Box<dyn Error>> {
    // let view = Assets::get_mut(last_view)?;
    // let image_mat = image_mat.as_standard_layout();
    //
    // // Preallocate space for the new vector
    // let mut new_image_mat = Vec::with_capacity(width * height * 3);
    //
    // let image_mat = image_mat.into_owned().into_raw_vec();
    // if color {
    //     // Iterate over chunks of 3 elements and insert the value after each chunk
    //     for chunk in image_mat.chunks(3) {
    //         new_image_mat.extend(chunk.iter().cloned());
    //     }
    // } else {
    //     for chunk in image_mat.chunks(1) {
    //         new_image_mat.extend(chunk.iter().cloned());
    //         new_image_mat.extend(chunk.iter().cloned());
    //         new_image_mat.extend(chunk.iter().cloned());
    //     }
    // }

    if !color {
        return Ok(ColorImage::from_gray(
            [width, height],
            image_mat.as_standard_layout().as_slice().unwrap(),
        ));
    } else {
        return Ok(ColorImage::from_rgb(
            [width, height],
            image_mat.as_standard_layout().as_slice().unwrap(),
        ));
    }

    panic!("Not implemented");
    // Ok(ColorImage::from_rgb(
    //     [width, height],
    //     new_image_mat.as_slice(),
    // ))
}

// pub fn prep_bevy_image_mut(
//     image_mat: Frame,
//     color: bool,
//     new_image: &mut Image,
// ) -> Result<(), Box<dyn Error>> {
//     let image_mat = image_mat.as_standard_layout().as_ptr();
//
//     let mut ref_idx = 0;
//     unsafe {
//         for (index, element) in new_image.data.iter_mut().enumerate() {
//             // Skip every 4th element
//             if (index + 1) % 4 == 0 {
//                 if !color {
//                     ref_idx += 1;
//                 }
//                 continue;
//             }
//
//             *element = *image_mat.offset(ref_idx);
//             if color {
//                 ref_idx += 1;
//             }
//         }
//     }
//
//     Ok(())
// }
