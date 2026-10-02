//! Synthetic search-only timing. Excludes discovery, pinyin preparation, UI, and app startup.
use picorun::{
    model::{AppEntry, LaunchTarget},
    search::{SearchEngine, MAX_RESULTS},
};
use std::{hint::black_box, path::PathBuf, time::Instant};

fn main() {
    println!("PicoRun synthetic warm search; release build required for useful timings.");
    println!("Excludes discovery, alias generation, UI, process memory and application launch.");
    let queries = [
        "",
        "微信",
        "weixin",
        "wx",
        "code",
        "vsc",
        "jishiben",
        "jsb",
        "music",
        "wyyyy",
        "App 99",
        "zqxvnomatch",
    ];
    for count in [500, 2000, 10000] {
        let titles = [
            "微信",
            "Visual Studio Code",
            "记事本",
            "网易云音乐",
            "Music Player",
        ];
        let entries: Vec<_> = (0..count)
            .map(|i| {
                AppEntry::new(
                    format!("{} App {i}", titles[i % titles.len()]),
                    LaunchTarget::ShellPath(PathBuf::from(format!("synthetic/{i}.lnk"))),
                )
            })
            .collect();
        let mut engine = SearchEngine::default();
        let mut hits = Vec::with_capacity(MAX_RESULTS);
        for _ in 0..20 {
            for query in queries {
                engine.search(&entries, query, &mut hits);
                black_box(&hits);
            }
        }
        let mut samples = Vec::with_capacity(1200);
        for _ in 0..100 {
            for query in queries {
                let start = Instant::now();
                engine.search(black_box(&entries), black_box(query), &mut hits);
                let elapsed = start.elapsed().as_nanos() as u64;
                black_box(&hits);
                samples.push(elapsed);
            }
        }
        samples.sort_unstable();
        println!(
            "entries={count} samples={} P50={:.4}ms P95={:.4}ms",
            samples.len(),
            samples[(samples.len() - 1) / 2] as f64 / 1_000_000.0,
            samples[((samples.len() * 95).div_ceil(100)).saturating_sub(1)] as f64 / 1_000_000.0
        );
    }
}
