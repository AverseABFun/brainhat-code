//! Template project for Flipper Zero.
//! This app prints "Hello, Rust!" to the console then exits.

#![no_main]
#![no_std]
#![allow(non_upper_case_globals)]

// Required for panic handler
extern crate flipperzero_rt;

// I want memory allocation
extern crate alloc;
extern crate flipperzero_alloc;

use core::ffi::c_void;

use flipperzero_sys::*;

struct Uart {
    rx_thread: *mut FuriThread,
    rx_stream: *mut FuriStreamBuffer,
    rx_buf: [u8; 321],
    handle_rx_data_cb: Option<fn(&Uart, &[u8], usize)>,
    baudrate: u32,
    handle: *mut FuriHalSerialHandle,
    adc: *mut FuriHalAdcHandle,
    exit: bool,
    input: u8,
}

enum WorkerEvtFlags {
    WorkerEvtStop = (1 << 0),
    WorkerEvtRxDone = (1 << 1),
}

unsafe extern "C" fn uart_worker(context: *mut core::ffi::c_void) -> i32 {
    let uart = &mut *(context as *mut Uart);

    loop {
        let events = furi_thread_flags_wait(
            WorkerEvtFlags::WorkerEvtStop as u32 | WorkerEvtFlags::WorkerEvtRxDone as u32,
            FuriFlagWaitAny.0,
            FuriWaitForever.0,
        );

        if (events & FuriFlagError.0) != 0 {
            continue;
        }

        if (events & WorkerEvtFlags::WorkerEvtStop as u32) != 0 {
            break;
        }
        if (events & WorkerEvtFlags::WorkerEvtRxDone as u32) != 0 {
            let len = furi_stream_buffer_receive(
                uart.rx_stream,
                (&mut uart.rx_buf) as *mut [u8; 321] as *mut c_void,
                320,
                10,
            );
            if len > 0 {
                if uart.handle_rx_data_cb.is_some() {
                    uart.handle_rx_data_cb.unwrap()(uart, &uart.rx_buf, len);
                }
            }
        }
    }

    furi_stream_buffer_free(uart.rx_stream);

    return 0;
}

unsafe extern "C" fn uart_irq(
    handle: *mut FuriHalSerialHandle,
    event: FuriHalSerialRxEvent,
    context: *mut core::ffi::c_void,
) {
    let uart = &mut *(context as *mut Uart);
    if event == FuriHalSerialRxEventData {
        let data = furi_hal_serial_async_rx(handle);
        furi_stream_buffer_send(uart.rx_stream, (&data) as *const u8 as *const c_void, 1, 0);
        furi_thread_flags_set(
            furi_thread_get_id(uart.rx_thread),
            WorkerEvtFlags::WorkerEvtRxDone as u32,
        );
    }
}

impl Uart {
    pub fn new(baudrate: u32) -> Uart {
        unsafe {
            furi_hal_serial_control_init();
            let serial_id = FuriHalSerialIdUsart;
            let handle = furi_hal_serial_control_acquire(serial_id);

            furi_hal_adc_init();

            let mut uart = Uart {
                rx_thread: furi_thread_alloc(),
                rx_stream: furi_stream_buffer_alloc(320, 1),
                rx_buf: [0; 321],
                handle_rx_data_cb: None,
                baudrate,
                handle,
                adc: furi_hal_adc_acquire(),
                exit: false,
                input: 0,
            };
            furi_hal_adc_configure_ex(
                uart.adc,
                FuriHalAdcScale2500,
                FuriHalAdcClockSync64,
                FuriHalAdcOversample4,
                FuriHalAdcSamplingtime6_5,
            );

            furi_thread_set_name(uart.rx_thread, c"BrainHat_UartRxThread".as_ptr());
            furi_thread_set_stack_size(uart.rx_thread, 1024);
            furi_thread_set_context(uart.rx_thread, (&mut uart) as *mut Uart as *mut c_void);
            furi_thread_set_callback(uart.rx_thread, Some(uart_worker));

            furi_thread_start(uart.rx_thread);

            furi_hal_serial_init(handle, baudrate);
            furi_hal_serial_async_rx_start(
                handle,
                Some(uart_irq),
                (&mut uart) as *mut Uart as *mut c_void,
                false,
            );

            uart
        }
    }
}

fn callback(uart: &Uart, buf: &[u8], len: usize) {
    if len < 2 {
        return;
    }

    let full_addr = buf[0];
    let rw = full_addr >> 7 == 1;
    let addr = full_addr & 0b1111111;

    let data_addr = buf[1];

    if addr != 0b1001000 {
        return;
    }

    unsafe {
        if rw {
            // Reading
            match data_addr {
                0 => {
                    // ADC
                    let out_data = furi_hal_adc_read(uart.adc, FuriHalAdcChannel2).to_be_bytes();
                    furi_hal_serial_tx(uart.handle, &out_data[0], out_data.len());
                }
                1 => { // Buttons
                }
                2 => {
                    // Status
                    let out_data = 0x4F4Bu16.to_be_bytes();
                    furi_hal_serial_tx(uart.handle, &out_data[0], out_data.len());
                }
                _ => {
                    panic!("Unknown data address");
                }
            }
        }
    }
}

unsafe extern "C" fn draw_callback(canvas: *mut Canvas, _context: *mut c_void) {
    //canvas_set_color(canvas, ColorBlack);
    //canvas_clear(canvas);
    canvas_set_color(canvas, ColorWhite);
    canvas_set_font(canvas, FontPrimary);
    let x: i32 = 64;
    let y: i32 = 32;
    let message = c"BrainHat running...".as_ptr();
    canvas_draw_str_aligned(canvas, x, y, AlignCenter, AlignCenter, message);
}

unsafe extern "C" fn input_callback(input_event: *mut InputEvent, context: *mut c_void) {
    furi_log_print_format(
        FuriLogLevelTrace,
        c"BrainHat".as_ptr(),
        c"Received input event".as_ptr(),
    );
    let uart = context as *mut Uart;
    if (*input_event).type_ == InputTypePress
        || (*input_event).type_ == InputTypeLong
        || (*input_event).type_ == InputTypeShort
    {
        match (*input_event).key {
            InputKeyUp => (*uart).input |= 1 << 0,
            InputKeyDown => (*uart).input |= 1 << 1,
            InputKeyLeft => (*uart).input |= 1 << 2,
            InputKeyRight => (*uart).input |= 1 << 3,
            InputKeyBack => {
                (*uart).exit = true;
                return;
            }
            _ => {}
        }
    } else if (*input_event).type_ == InputTypeRelease {
        match (*input_event).key {
            InputKeyUp => (*uart).input &= !(1 << 0),

            InputKeyDown => (*uart).input &= !(1 << 1),
            InputKeyLeft => (*uart).input &= !(1 << 2),
            InputKeyRight => (*uart).input &= !(1 << 3),
            InputKeyBack => {
                (*uart).exit = true;
                return;
            }
            _ => {}
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn rust_main(_args: *const c_void) -> i32 {
    let mut uart = Uart::new(115200);
    uart.handle_rx_data_cb = Some(callback);

    let view_port = view_port_alloc();
    view_port_draw_callback_set(
        view_port,
        Some(draw_callback),
        &mut uart as *mut Uart as *mut c_void,
    );
    view_port_input_callback_set(
        view_port,
        Some(input_callback),
        &mut uart as *mut Uart as *mut c_void,
    );

    furi_log_print_format(
        FuriLogLevelTrace,
        c"BrainHat".as_ptr(),
        c"ViewPort set up".as_ptr(),
    );

    let gui = furi_record_open(c"gui".as_ptr()) as *mut Gui;
    gui_add_view_port(gui, view_port, GuiLayerFullscreen);
    view_port_enabled_set(view_port, true);

    furi_log_print_format(
        FuriLogLevelTrace,
        c"BrainHat".as_ptr(),
        c"Gui set up".as_ptr(),
    );
    furi_log_print_format(
        FuriLogLevelTrace,
        c"BrainHat".as_ptr(),
        c"Loop starting".as_ptr(),
    );

    while !uart.exit {}
    0
}
