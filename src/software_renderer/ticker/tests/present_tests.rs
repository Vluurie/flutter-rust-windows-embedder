use crate::software_renderer::ticker::present::copy_framebuffer;

#[test]
fn copies_all_rows_same_pitch() {
    let src: Vec<u8> = (0..12u8).collect();
    let mut dst = vec![0u8; 12];
    copy_framebuffer(&src, &mut dst, 3, 4, 4, 4, 12);
    assert_eq!(dst, src);
}

#[test]
fn copies_with_different_pitches() {
    let src = vec![1, 2, 3, 4, 5, 6, 7, 8];
    let mut dst = vec![0u8; 12];
    copy_framebuffer(&src, &mut dst, 2, 4, 6, 4, 12);
    assert_eq!(&dst[0..4], &[1, 2, 3, 4]);
    assert_eq!(&dst[4..6], &[0, 0]);
    assert_eq!(&dst[6..10], &[5, 6, 7, 8]);
}

#[test]
fn zero_rows_copies_nothing() {
    let src = vec![1u8, 2, 3, 4];
    let mut dst = vec![9u8; 4];
    copy_framebuffer(&src, &mut dst, 0, 4, 4, 4, 4);
    assert_eq!(dst, vec![9, 9, 9, 9]);
}

#[test]
fn short_dst_does_not_overrun() {
    let src: Vec<u8> = (0..16u8).collect();
    let mut dst = vec![0u8; 8];
    copy_framebuffer(&src, &mut dst, 4, 4, 4, 4, 8);
    assert_eq!(&dst[0..4], &[0, 1, 2, 3]);
    assert_eq!(&dst[4..8], &[4, 5, 6, 7]);
}

#[test]
fn short_src_does_not_overrun() {
    let src = vec![1u8, 2, 3, 4];
    let mut dst = vec![0u8; 16];
    copy_framebuffer(&src, &mut dst, 4, 4, 4, 4, 16);
    assert_eq!(&dst[0..4], &[1, 2, 3, 4]);
    assert_eq!(&dst[4..8], &[0, 0, 0, 0]);
}
