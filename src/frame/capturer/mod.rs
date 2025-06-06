pub mod none;
pub mod wayland;

#[allow(clippy::large_enum_variant)]
pub enum Capturer {
    None(none::Capturer),
    Wayland(wayland::Capturer),
}

impl Capturer {
    pub async fn run(&mut self, output_name: &str, controller: crate::predictor::Controller) {
        match self {
            Self::None(c) => c.run(output_name, controller).await,
            Self::Wayland(c) => {
                // SAFETY: here, we cast all values we pass as a reference to the closure to raw
                // pointers and then to usizes, reverting this in the closure. We do this as the
                // closure is sent to another thread. This makes the borrow checker believe that
                // all captured references of the closure must be 'static, but we assert that the
                // closure is dead before this function returns by `await`-ing the future returned
                // by `smol::unblock`. The reason we don't pass raw pointers (which don't have
                // lifetimes into the closure directly is that those don't impl `Send`, in fact,
                // they're even explicitly `!Send`).
                let c_ptr = c as *mut wayland::Capturer as usize;
                let output_name_ptr = output_name.as_ptr() as usize;
                let output_name_len = output_name.len();
                smol::unblock(move || unsafe {
                    // TODO: make the wayland capturer async instead of using unblock to run it in
                    // a thread pool (to avoid blocking the runtime).
                    (*(c_ptr as *mut wayland::Capturer)).run(
                        std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                            output_name_ptr as *const u8,
                            output_name_len,
                        )),
                        controller,
                    )
                })
                .await;
            }
        }
    }
}
