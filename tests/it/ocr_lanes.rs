use pdf_inspector::vision::run_in_lanes;
use rayon::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::Duration;

const LANES: usize = 3;
const ROUNDS: usize = 60;

#[test]
fn a_lane_holding_its_lock_while_running_rayon_work_is_never_given_another_page() {
    let lane_locks: Vec<Mutex<()>> = (0..LANES).map(|_| Mutex::new(())).collect();
    let reentered = AtomicUsize::new(0);
    let pages: Vec<usize> = (0..9).collect();

    for _ in 0..ROUNDS {
        let done = run_in_lanes(&pages, LANES, |lane, &page| {
            let Ok(_held) = lane_locks[lane].try_lock() else {
                reentered.fetch_add(1, Ordering::Relaxed);
                return page;
            };
            (0..48)
                .into_par_iter()
                .for_each(|_| std::thread::sleep(Duration::from_micros(300)));
            page
        });
        assert_eq!(done, pages, "every page comes back once, in page order");
    }

    assert_eq!(
        reentered.load(Ordering::Relaxed),
        0,
        "a lane was handed a page while its own lock was held, which is the OCR deadlock"
    );
}

#[test]
fn every_page_is_given_to_exactly_one_lane() {
    let calls = AtomicUsize::new(0);
    let pages: Vec<usize> = (0..10).collect();

    let done = run_in_lanes(&pages, LANES, |lane, &page| {
        calls.fetch_add(1, Ordering::Relaxed);
        assert!(lane < LANES, "lane {lane} is past the worker count");
        page * 2
    });

    assert_eq!(calls.load(Ordering::Relaxed), pages.len());
    assert_eq!(done, pages.iter().map(|p| p * 2).collect::<Vec<_>>());
}

#[test]
fn no_pages_means_no_work() {
    let done = run_in_lanes(&Vec::<usize>::new(), LANES, |_, &page| page);
    assert!(done.is_empty());
}
