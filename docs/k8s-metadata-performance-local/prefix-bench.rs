//! Standalone candidate-3 experiment; no production VRL changes.
use std::{borrow::Borrow, cmp::Ordering, collections::BTreeMap, hint::black_box, time::Instant};

#[derive(Clone, Eq, PartialEq)]
struct Cached<const N: usize> { text: String, prefix: [u64; N] }
impl<const N: usize> From<String> for Cached<N> {
    fn from(text: String) -> Self {
        let mut prefix = [0; N];
        for (word, chunk) in prefix.iter_mut().zip(text.as_bytes().chunks(8)) {
            let mut bytes = [0; 8];
            bytes[..chunk.len()].copy_from_slice(chunk);
            *word = u64::from_be_bytes(bytes);
        }
        Self { text, prefix }
    }
}
impl<const N: usize> Ord for Cached<N> {
    fn cmp(&self, other: &Self) -> Ordering {
        // Full fallback also distinguishes short strings containing NUL bytes.
        self.prefix.cmp(&other.prefix).then_with(|| self.text.cmp(&other.text))
    }
}
impl<const N: usize> PartialOrd for Cached<N> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> { Some(self.cmp(other)) }
}
impl<const N: usize> Borrow<str> for Cached<N> {
    fn borrow(&self) -> &str { &self.text }
}

fn validate<const N: usize>() {
    let mut strings = vec![String::new(), "a".into(), "a\0".into(), "a\0\0".into(), "é".into(), "😀".into()];
    let mut state = 12345u64;
    for len in 0..80 {
        for _ in 0..5 {
            let s: String = (0..len).map(|_| {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                char::from((state >> 32) as u8)
            }).collect();
            strings.push(s);
        }
    }
    for a in &strings { for b in &strings {
        assert_eq!(a.cmp(b), Cached::<N>::from(a.clone()).cmp(&Cached::<N>::from(b.clone())));
    }}
    let expected: BTreeMap<_, _> = strings.iter().cloned().enumerate().map(|(i, s)| (s, i)).collect();
    let actual: BTreeMap<Cached<N>, _> = strings.iter().cloned().enumerate().map(|(i, s)| (s.into(), i)).collect();
    for s in &strings { assert_eq!(expected.get(s.as_str()), actual.get(s.as_str())); }
    assert_eq!(expected.keys().map(String::as_str).collect::<Vec<_>>(), actual.keys().map(|k| k.text.as_str()).collect::<Vec<_>>());
}

fn measure<K: From<String> + Ord + Borrow<str>>(keys: &[String], collect: bool) -> f64 {
    let start = Instant::now();
    for _ in 0..10_000 {
        let map: BTreeMap<K, usize> = if collect {
            keys.iter().enumerate().map(|(i, key)| (K::from(key.clone()), i)).collect()
        } else {
            let mut map = BTreeMap::new();
            for (i, key) in keys.iter().enumerate() { map.insert(K::from(key.clone()), i); }
            map
        };
        // Borrowed lookup intentionally bypasses cached ordering, as in VRL.
        for key in keys.iter().step_by(3) { black_box(map.get(black_box(key.as_str()))); }
        black_box(map.get(black_box("missing-key")));
        drop(black_box(map));
    }
    start.elapsed().as_nanos() as f64 / 10_000.0
}

fn main() {
    validate::<1>(); validate::<2>(); validate::<4>();
    eprintln!("sizes String={} cached8={} cached16={} cached32={}", std::mem::size_of::<String>(), std::mem::size_of::<Cached<1>>(), std::mem::size_of::<Cached<2>>(), std::mem::size_of::<Cached<4>>());
    println!("style,keys,collect,pair,string_ns,prefix8_ns,prefix16_ns,prefix32_ns");
    for style in ["short", "diverse", "k8s", "long_shared"] {
        for count in [1, 10, 21, 50, 100] {
            let mut keys: Vec<_> = (0..count).map(|i| match style {
                "short" => format!("k{i:03}"),
                "diverse" => format!("{i:03}.example.org/metadata-field"),
                "k8s" => format!("bench.example/key{i:02}"),
                _ => format!("app.kubernetes.io/very-long-common-prefix/field{i:03}"),
            }).collect();
            keys.sort();
            for collect in [false, true] {
                for pair in 0..5 {
                    let mut times = [0.0; 4];
                    let order = if pair % 2 == 0 { [0, 1, 2, 3] } else { [3, 2, 1, 0] };
                    for index in order {
                        times[index] = match index {
                            0 => measure::<String>(&keys, collect),
                            1 => measure::<Cached<1>>(&keys, collect),
                            2 => measure::<Cached<2>>(&keys, collect),
                            _ => measure::<Cached<4>>(&keys, collect),
                        };
                    }
                    println!("{style},{count},{collect},{pair},{:.2},{:.2},{:.2},{:.2}", times[0], times[1], times[2], times[3]);
                }
            }
        }
    }
}
