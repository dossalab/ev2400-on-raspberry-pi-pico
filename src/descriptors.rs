use usbd_hid::descriptor::gen_hid_descriptor;
use usbd_hid::descriptor::generator_prelude::*;

#[gen_hid_descriptor(
     (collection = APPLICATION, usage_page = 0xFF09, usage = 1) = {
        (report_id = 1, usage = 1) = {
            buff1=input
        };
        (report_id = 0x3F, usage = 1) = {
            buff2=output
        };
     }
)]
#[allow(dead_code)]
pub struct TiHidReport {
    pub buff1: [u8; 62],
    pub buff2: [u8; 62],
}
