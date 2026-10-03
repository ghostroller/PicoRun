//! Explicit --measure-icons probes. They preserve normal focus, keyboard, hotkey and painting.
use super::*;
thread_local! { static DRAW_P95: Cell<usize> = const { Cell::new(0) }; }
pub(super) unsafe fn message(hwnd: Hwnd, msg: u32, wp: usize) -> Option<isize> {
    Some(match msg {
        0x8006 => state(|s| {
            if wp == 17 {
                return usize::from(s.view.show_icons);
            }
            if wp == 18 {
                return usize::from(s.icons.as_ref().is_some_and(|i| i.started()));
            }
            if wp == 6 {
                return s.view.icons.iter().filter(|i| i.is_some()).count();
            }
            let Some(session) = &s.icons else {
                return usize::from(wp == 0);
            };
            if wp == 0 {
                return usize::from(session.requested.get() == session.completed.get());
            }
            let stats = session.stats.borrow();
            match wp {
                1 => stats.cache,
                2 => stats.misses,
                3 => stats.hits,
                4 => stats.max_load_us,
                5 => stats.total_load_us,
                7 => stats.fallbacks,
                8 => stats.parses,
                9 => stats.metadata_hits,
                10 => stats.metadata_entries,
                11 => stats.metadata_bytes,
                12 => stats.generic_copies,
                13 => stats.shared_resources,
                14 => stats.extracts,
                15 => stats.invalidations,
                _ => 0,
            }
        })
        .unwrap_or(0) as isize,
        0x8007 => {
            let name = state(|s| {
                s.controller
                    .catalog()
                    .entries()
                    .get(wp)
                    .map(|e| e.name.clone())
            })
            .flatten()?;
            cancel_composition();
            SetWindowTextW(EDIT.get(), wide(&name).as_ptr());
            sync_query();
            UpdateWindow(hwnd);
            state(|s| isize::from(s.text == name)).unwrap_or(0)
        }
        0x8008 => state(|s| s.controller.catalog().entries().len()).unwrap_or(0) as isize,
        0x8009 => {
            if wp == 1 {
                return Some(DRAW_P95.get() as isize);
            }
            for _ in 0..20 {
                InvalidateRect(hwnd, null(), 0);
                SendMessageW(hwnd, 0xf, 0, 0);
            }
            let mut samples = Vec::with_capacity(200);
            for _ in 0..200 {
                let started = std::time::Instant::now();
                InvalidateRect(hwnd, null(), 0);
                SendMessageW(hwnd, 0xf, 0, 0);
                samples.push(started.elapsed().as_micros() as usize);
            }
            samples.sort_unstable();
            DRAW_P95.set(samples[189]);
            samples[99] as isize
        }
        0x800a => {
            hide();
            0
        }
        _ => return None,
    })
}
