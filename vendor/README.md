# Vendored crates

## pdf-inspector 1.26.0

This is the crates.io release of
[pdf-inspector](https://github.com/firecrawl/pdf-inspector) 1.26.0, unchanged
except for one function: `OcrEngine::recognize` for `OarOcrEngine` in
`src/vision/oar.rs`. `Cargo.toml` points at it through `[patch.crates-io]`.

Remove this copy and the patch entry when an upstream release fixes the
deadlock below. 1.27.1 does not: its `recognize` is the same code.

### The deadlock

OCR of a scanned PDF with more than one page can hang forever with every
thread idle. It is a race, so it is rare per page and certain over a large
corpus: `rhebok index` hung on a real corpus of 102 scanned PDFs twice, after
71 and 30 minutes of CPU time. The second time was a reproduction with
symbols.

The cause is two locks that are each correct alone:

1. pdf-inspector recognizes pages with `par_iter` on its own rayon pool, and
   each page picks its OCR worker by `rayon::current_thread_index()`. Each
   worker owns its ONNX sessions.
2. oar-ocr 0.9.2 holds a session's `Mutex` while it runs the callback of
   `infer_first_output_f32`. In `CRNNModel::forward_refs` that callback is the
   CTC argmax, which runs `par_chunks_exact` (`processors/decode.rs`).

A rayon thread that waits for the other half of that argmax steals work. If
it steals a page job, that page runs on the same thread, so it picks the same
worker and locks the same session again. `std::sync::Mutex` is not
reentrant, so the thread blocks on itself, and every other page waits on it.

The sampled stack of the hung thread, in call order:

    rayon worker -> page job (par_iter over pages)
      -> OarOcrEngine::recognize_page -> TextRecognitionPredictor::predict
      -> CRNNModel::forward_refs             (session lock taken)
      -> par_chunks_exact argmax -> join -> wait_until_cold
      -> steals a page job -> recognize_page -> predict
      -> CRNNModel::forward_refs             (blocks on the same lock)

### The fix

Pages are no longer rayon jobs. `recognize` calls `run_in_lanes`, which starts
one scoped thread for each worker. Each thread takes the next page index from
a shared counter and always uses its own worker. A thread that waits inside
rayon can then only steal argmax work, which takes no session lock. Results go
back in page order, and the first error in page order is returned, as before.

`run_in_lanes` is public, and hidden from the docs, so that rhebok can test it.
`tests/it/ocr_lanes.rs` gives each lane a lock that it takes with `try_lock`,
and runs rayon work while it holds the lock. A lane that gets a second page
while it holds its lock fails the test at once, instead of hanging. With the
old rayon scheduler put back into `run_in_lanes`, the test failed 8 runs of 8.

The pool is still built and still decides whether recognition is parallel,
but recognition no longer installs it. The inner rayon work now runs on the
global pool.
