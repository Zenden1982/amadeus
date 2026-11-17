//! Совместимость с предкомпилированным libwebrtc (LiveKit собирает его MSVC 17.14+).
//!
//! Он ссылается на `__std_find_first_of_trivial_pos_1` из STL 14.44, а на ПК стоит MSVC 14.43.
//! Здесь — эквивалентная реализация (семантика из vector_algorithms.cpp STL: индекс первого байта
//! из [first1, first1+count1), входящего в набор [first2, first2+count2), иначе SIZE_MAX).
//! **Удалить после обновления VS Build Tools до 17.14+** (иначе будет дубль символа, LNK2005).

#[cfg(all(windows, target_env = "msvc"))]
#[unsafe(no_mangle)]
pub unsafe extern "C" fn __std_find_first_of_trivial_pos_1(
    first1: *const u8,
    count1: usize,
    first2: *const u8,
    count2: usize,
) -> usize {
    if count1 == 0 || count2 == 0 {
        return usize::MAX;
    }
    // SAFETY: STL передаёт валидные диапазоны указанной длины.
    let (hay, set) = unsafe { (std::slice::from_raw_parts(first1, count1), std::slice::from_raw_parts(first2, count2)) };
    let mut table = [false; 256];
    for &b in set {
        table[b as usize] = true;
    }
    hay.iter().position(|&b| table[b as usize]).unwrap_or(usize::MAX)
}

#[cfg(all(test, windows, target_env = "msvc"))]
mod tests {
    #[test]
    fn find_first_of() {
        let hay = b"hello, world";
        let f = |set: &[u8]| unsafe { super::__std_find_first_of_trivial_pos_1(hay.as_ptr(), hay.len(), set.as_ptr(), set.len()) };
        assert_eq!(f(b"ol"), 2);
        assert_eq!(f(b",w"), 5);
        assert_eq!(f(b"xyz"), usize::MAX);
        assert_eq!(f(b""), usize::MAX);
    }
}
