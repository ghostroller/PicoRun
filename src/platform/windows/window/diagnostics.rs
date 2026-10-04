//! Explicit --measure-icons probes. They preserve normal focus, keyboard, hotkey and painting.
use super::*;
thread_local! { static DRAW_P95: Cell<usize> = const { Cell::new(0) }; }
thread_local! {
    static ARROW_REGION: Cell<[i32; 5]> = const { Cell::new([0; 5]) };
    static EDIT_PAINTS: Cell<usize> = const { Cell::new(0) };
}
pub(super) fn edit_paint() {
    EDIT_PAINTS.set(EDIT_PAINTS.get() + 1);
}
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
        0x800b => {
            UpdateWindow(hwnd);
            UpdateWindow(EDIT.get());
            EDIT_PAINTS.set(0);
            SendMessageW(EDIT.get(), 0x100, wp, 1);
            SendMessageW(EDIT.get(), 0x101, wp, 1);
            let mut rect = Rect::default();
            GetUpdateRect(hwnd, &mut rect, 0);
            ARROW_REGION.set([
                rect.left,
                rect.top,
                rect.right,
                rect.bottom,
                GetUpdateRect(EDIT.get(), null_mut(), 0),
            ]);
            UpdateWindow(hwnd);
            UpdateWindow(EDIT.get());
            state(|s| s.controller.selected_index())
                .flatten()
                .map_or(-1, |i| i as isize)
        }
        0x800c => {
            if wp == 5 {
                EDIT_PAINTS.get() as isize
            } else if (7..=10).contains(&wp) {
                renderer().map_or(0, |r| match wp {
                    7 => r.dpi as isize,
                    8 => r.scale(r.theme.row_height) as isize,
                    9 => r.top() as isize,
                    _ => r.scale(r.theme.width) as isize,
                })
            } else if wp == 6 {
                state(|s| s.controller.selected_index())
                    .flatten()
                    .map_or(-1, |i| i as isize)
            } else if wp == 12 {
                renderer().map_or(0, |r| r.row_buffer_pixels() as isize)
            } else if wp == 13 {
                renderer().map_or(0, |r| r.edit_buffer_bytes() as isize)
            } else {
                *ARROW_REGION.get().get(wp).unwrap_or(&0) as isize
            }
        }
        0x800d => {
            if !(96..=384).contains(&wp) {
                return Some(0);
            }
            let mut rect = Rect::default();
            GetWindowRect(hwnd, &mut rect);
            // A stack pointer is valid here: this diagnostic sends within the owning process.
            SendMessageW(hwnd, 0x2e0, wp | wp << 16, &rect as *const Rect as isize);
            UpdateWindow(hwnd);
            1
        }
        0x800e => {
            // Scalar hashes avoid cross-process pointers. Available only with --measure-icons.
            fn hash(units: impl Iterator<Item = u16>) -> isize {
                units.fold(2166136261u32, |value, unit| {
                    (value ^ u32::from(unit)).wrapping_mul(16777619)
                }) as isize
            }
            match wp {
                0 => isize::from(i18n::current() == Language::English),
                1 => hash(view()?.status.iter().copied()),
                2 => hash(view()?.help.iter().copied().take_while(|unit| *unit != 0)),
                3 => hash(view()?.empty.iter().copied().take_while(|unit| *unit != 0)),
                4 => hash(
                    view()?
                        .rows
                        .iter()
                        .flat_map(|row| row.iter().copied().chain(Some(0))),
                ),
                5 => hash(view()?.cue.iter().copied().take_while(|unit| *unit != 0)),
                _ => 0,
            }
        }
        _ => return None,
    })
}
