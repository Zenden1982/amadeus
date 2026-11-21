//! QP кадров H.264 из заголовков срезов (диагностика качества: аппаратные энкодеры QP наружу не отдают).
//! QP = 26 + pic_init_qp_minus26 (PPS) + slice_qp_delta (заголовок первого среза кадра).
//! Разбор по ITU-T H.264 §7.3.2.1 (SPS), §7.3.2.2 (PPS), §7.3.3 (slice header). Матрицы квантования и группы срезов
//! не поддерживаются — тогда QP неизвестен (None).

struct Bits<'a> {
    d: &'a [u8],
    pos: usize,
}

impl<'a> Bits<'a> {
    fn new(d: &'a [u8]) -> Self {
        Self { d, pos: 0 }
    }

    fn bit(&mut self) -> Option<u32> {
        let byte = *self.d.get(self.pos / 8)?;
        let b = (byte >> (7 - self.pos % 8)) & 1;
        self.pos += 1;
        Some(b as u32)
    }

    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut v = 0u32;
        for _ in 0..n {
            v = (v << 1) | self.bit()?;
        }
        Some(v)
    }

    fn flag(&mut self) -> Option<bool> {
        Some(self.bit()? == 1)
    }

    fn ue(&mut self) -> Option<u32> {
        let mut zeros = 0;
        while self.bit()? == 0 {
            zeros += 1;
            if zeros > 31 {
                return None;
            }
        }
        Some(((1u64 << zeros) - 1 + self.bits(zeros)? as u64) as u32)
    }

    fn se(&mut self) -> Option<i32> {
        let k = self.ue()? as i64;
        Some(if k % 2 == 1 { ((k + 1) / 2) as i32 } else { (-(k / 2)) as i32 })
    }
}

/// Убрать байты защиты от эмуляции (00 00 03 → 00 00).
fn rbsp(nal: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(nal.len());
    let mut zeros = 0;
    for &b in nal {
        if zeros >= 2 && b == 3 {
            zeros = 0;
            continue;
        }
        zeros = if b == 0 { zeros + 1 } else { 0 };
        out.push(b);
    }
    out
}

/// NAL-блоки Annex B (без стартовых кодов).
pub fn nals(data: &[u8]) -> Vec<&[u8]> {
    let mut starts = Vec::new();
    let mut i = 0;
    while i + 3 <= data.len() {
        if data[i] == 0 && data[i + 1] == 0 && data[i + 2] == 1 {
            starts.push(i + 3);
            i += 3;
        } else {
            i += 1;
        }
    }
    let mut out = Vec::with_capacity(starts.len());
    for (k, &s) in starts.iter().enumerate() {
        let mut e = starts.get(k + 1).map(|n| n - 3).unwrap_or(data.len());
        // 4-байтный стартовый код: лишний ноль в конце предыдущего блока
        while e > s && data[e - 1] == 0 {
            e -= 1;
        }
        if e > s {
            out.push(&data[s..e]);
        }
    }
    out
}

#[derive(Clone, Copy)]
struct Sps {
    chroma_format_idc: u32,
    separate_colour_plane: bool,
    log2_max_frame_num: u32,
    poc_type: u32,
    log2_max_poc_lsb: u32,
    delta_pic_order_always_zero: bool,
    frame_mbs_only: bool,
}

#[derive(Clone, Copy)]
struct Pps {
    cabac: bool,
    bottom_field_pic_order: bool,
    num_ref_idx_l0_default: u32,
    num_ref_idx_l1_default: u32,
    weighted_pred: bool,
    weighted_bipred_idc: u32,
    pic_init_qp: i32,
    redundant_pic_cnt_present: bool,
}

fn parse_sps(r: &mut Bits) -> Option<Sps> {
    let profile = r.bits(8)?;
    r.bits(16)?; // constraint flags + level
    r.ue()?; // sps id
    let (mut chroma_format_idc, mut separate_colour_plane) = (1, false);
    if matches!(profile, 100 | 110 | 122 | 244 | 44 | 83 | 86 | 118 | 128 | 138 | 139 | 134 | 135) {
        chroma_format_idc = r.ue()?;
        if chroma_format_idc == 3 {
            separate_colour_plane = r.flag()?;
        }
        r.ue()?;
        r.ue()?;
        r.flag()?;
        if r.flag()? {
            return None; // матрицы квантования — не поддерживаем
        }
    }
    let log2_max_frame_num = r.ue()? + 4;
    let poc_type = r.ue()?;
    let (mut log2_max_poc_lsb, mut delta_pic_order_always_zero) = (0, false);
    match poc_type {
        0 => log2_max_poc_lsb = r.ue()? + 4,
        1 => {
            delta_pic_order_always_zero = r.flag()?;
            r.se()?;
            r.se()?;
            for _ in 0..r.ue()? {
                r.se()?;
            }
        }
        _ => {}
    }
    r.ue()?; // max_num_ref_frames
    r.flag()?;
    r.ue()?;
    r.ue()?;
    let frame_mbs_only = r.flag()?;
    Some(Sps { chroma_format_idc, separate_colour_plane, log2_max_frame_num, poc_type, log2_max_poc_lsb, delta_pic_order_always_zero, frame_mbs_only })
}

fn parse_pps(r: &mut Bits) -> Option<Pps> {
    r.ue()?;
    r.ue()?;
    let cabac = r.flag()?;
    let bottom_field_pic_order = r.flag()?;
    if r.ue()? > 0 {
        return None; // группы срезов — не поддерживаем
    }
    let num_ref_idx_l0_default = r.ue()? + 1;
    let num_ref_idx_l1_default = r.ue()? + 1;
    let weighted_pred = r.flag()?;
    let weighted_bipred_idc = r.bits(2)?;
    let pic_init_qp = 26 + r.se()?;
    r.se()?; // pic_init_qs
    r.se()?; // chroma_qp_index_offset
    r.flag()?; // deblocking_filter_control_present
    r.flag()?; // constrained_intra_pred
    let redundant_pic_cnt_present = r.flag()?;
    Some(Pps { cabac, bottom_field_pic_order, num_ref_idx_l0_default, num_ref_idx_l1_default, weighted_pred, weighted_bipred_idc, pic_init_qp, redundant_pic_cnt_present })
}

fn ref_list_modification(r: &mut Bits) -> Option<()> {
    if r.flag()? {
        loop {
            match r.ue()? {
                0..=2 => {
                    r.ue()?;
                }
                3 => break,
                _ => return None,
            }
        }
    }
    Some(())
}

fn pred_weight_table(r: &mut Bits, sps: &Sps, l0: u32, l1: Option<u32>) -> Option<()> {
    r.ue()?;
    let chroma = sps.chroma_format_idc != 0 && !sps.separate_colour_plane;
    if chroma {
        r.ue()?;
    }
    for n in std::iter::once(l0).chain(l1) {
        for _ in 0..n {
            if r.flag()? {
                r.se()?;
                r.se()?;
            }
            if chroma && r.flag()? {
                for _ in 0..4 {
                    r.se()?;
                }
            }
        }
    }
    Some(())
}

fn slice_qp(r: &mut Bits, nal_type: u8, nal_ref_idc: u8, sps: &Sps, pps: &Pps) -> Option<i32> {
    r.ue()?; // first_mb_in_slice
    let st = r.ue()? % 5; // 0 P, 1 B, 2 I, 3 SP, 4 SI
    r.ue()?; // pps id
    if sps.separate_colour_plane {
        r.bits(2)?;
    }
    r.bits(sps.log2_max_frame_num)?;
    let mut field = false;
    if !sps.frame_mbs_only {
        field = r.flag()?;
        if field {
            r.flag()?;
        }
    }
    if nal_type == 5 {
        r.ue()?;
    }
    if sps.poc_type == 0 {
        r.bits(sps.log2_max_poc_lsb)?;
        if pps.bottom_field_pic_order && !field {
            r.se()?;
        }
    }
    if sps.poc_type == 1 && !sps.delta_pic_order_always_zero {
        r.se()?;
        if pps.bottom_field_pic_order && !field {
            r.se()?;
        }
    }
    if pps.redundant_pic_cnt_present {
        r.ue()?;
    }
    if st == 1 {
        r.flag()?; // direct_spatial_mv_pred
    }
    let (mut l0, mut l1) = (pps.num_ref_idx_l0_default, pps.num_ref_idx_l1_default);
    if matches!(st, 0 | 1 | 3) && r.flag()? {
        l0 = r.ue()? + 1;
        if st == 1 {
            l1 = r.ue()? + 1;
        }
    }
    if !matches!(st, 2 | 4) {
        ref_list_modification(r)?;
        if st == 1 {
            ref_list_modification(r)?;
        }
    }
    if (pps.weighted_pred && matches!(st, 0 | 3)) || (pps.weighted_bipred_idc == 1 && st == 1) {
        pred_weight_table(r, sps, l0, (st == 1).then_some(l1))?;
    }
    if nal_ref_idc != 0 {
        if nal_type == 5 {
            r.flag()?;
            r.flag()?;
        } else if r.flag()? {
            loop {
                match r.ue()? {
                    0 => break,
                    // 3: difference_of_pic_nums и long_term_frame_idx
                    3 => {
                        r.ue()?;
                        r.ue()?;
                    }
                    1 | 2 | 4 | 6 => {
                        r.ue()?;
                    }
                    5 => {}
                    _ => return None,
                }
            }
        }
    }
    if pps.cabac && !matches!(st, 2 | 4) {
        r.ue()?;
    }
    Some(pps.pic_init_qp + r.se()?)
}

/// Помнит последние SPS/PPS и выдаёт QP кадра (первого среза).
#[derive(Default)]
pub struct QpParser {
    sps: Option<Sps>,
    pps: Option<Pps>,
}

impl QpParser {
    /// Кадр в Annex B (может содержать SPS/PPS). None — QP не найден.
    pub fn frame_qp(&mut self, data: &[u8]) -> Option<i32> {
        for nal in nals(data) {
            let Some(&h) = nal.first() else { continue };
            let (ty, ref_idc) = (h & 0x1F, (h >> 5) & 3);
            match ty {
                7 => self.sps = parse_sps(&mut Bits::new(&rbsp(&nal[1..]))),
                8 => self.pps = parse_pps(&mut Bits::new(&rbsp(&nal[1..]))),
                1 | 5 => {
                    let (sps, pps) = (self.sps?, self.pps?);
                    // хватит начала среза — заголовок короткий
                    let head = &nal[1..nal.len().min(64)];
                    return slice_qp(&mut Bits::new(&rbsp(head)), ty, ref_idc, &sps, &pps).filter(|q| (0..=51).contains(q));
                }
                _ => {}
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Поток из N кадров: разбить по кадрам (каждый срез 1/5 — новый кадр) и прогнать через парсер.
    fn qps(file: &str) -> Vec<i32> {
        let data = std::fs::read(format!("{}/tests/data/{file}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let mut p = QpParser::default();
        let mut out = Vec::new();
        let mut pending: Vec<u8> = Vec::new();
        for nal in nals(&data) {
            pending.extend_from_slice(&[0, 0, 0, 1]);
            pending.extend_from_slice(nal);
            if matches!(nal[0] & 0x1F, 1 | 5) {
                out.push(p.frame_qp(&pending).expect("QP не разобран"));
                pending.clear();
            }
        }
        out
    }

    #[test]
    fn x264_baseline_constant_qp() {
        let v = qps("x264_base_qp31.264");
        assert_eq!(v.len(), 8);
        assert!(v.iter().all(|q| *q == 31), "{v:?}");
    }

    #[test]
    fn x264_main_cabac_bframes() {
        let v = qps("x264_main_qp22.264");
        assert_eq!(v.len(), 8);
        assert!(v.iter().all(|q| *q == 22), "{v:?}");
    }

    #[test]
    fn nvenc_baseline_constqp() {
        // ffmpeg nvenc при -qp 27 даёт ключевому кадру QP ниже (i_quant_factor) — 22, остальным ровно 27
        let v = qps("nvenc_base_qp27.264");
        assert_eq!(v.len(), 8);
        assert_eq!(v[0], 22, "{v:?}");
        assert!(v[1..].iter().all(|q| *q == 27), "{v:?}");
    }

    #[test]
    fn emulation_prevention() {
        assert_eq!(rbsp(&[0, 0, 3, 1, 0, 0, 3, 0]), vec![0, 0, 1, 0, 0, 0]);
    }
}
