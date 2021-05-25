use std::fs::File;
use std::io;
use std::io::{stdout, BufWriter, Write};
use std::process::exit;
use std::time::Duration;

use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};

const HELP: &str = "\
Simple Serial Terminal TUI in the shell 
USAGE:
	app [OPTIONS]

    String values for options are always case insensitive
FLAGS:
	-h, --help            Prints help information
OPTIONS:
	-p --port         STRING     Serial port (/dev/tty* or COMx)
        Not needed if there is only one port
        If not given and there are multiple ports we will just enumerate the ports
	-b --baud-rate    NUMBER     Baud rate to open with [ default: 115200 ]
	-d --data-bits    NUMBER     Data bits (5,6,7,8) [ default: 8 ]
	-F --flow-control STRING     Flow control for the port (None, SW, HW) [default: None]
	-P --parity       STRING     Which parity to use (None, odd, even) [default: None]
	-s --stop-bits    NUMBER     Number of stop bits (1, 2) [default: 1]
	-T --timeout      NUMBER     Timeout (milliseconds) on receiving data [default: 0]
    -m --mode         STRING     stdout, iterm, lines, file  [default: stdout]
    -o --output-file  PATH       File to write to (only relevant with `-m file`) (unimplemented)
    -c --capacity     NUMBER     Buffer capacity for stdout/file writing [default: 64] 
        This is set quite low to be able to see live updates (and because UART is slow usually)
        You could increase this a lot if you are writing to a file and/or using faster data
    -f --format       PATH       Path to file with parser format (unimplemented)
        Parse binary data into human-readable format for more efficient bandwidth usage
";

#[derive(Debug)]
struct Args {
    port: String,
    baud_rate: u32,
    data_bits: DataBits,
    flow_control: FlowControl,
    parity: Parity,
    stop_bits: StopBits,
    timeout: Duration,
    mode: String,
    capacity: usize,
}
impl Default for Args {
    fn default() -> Self {
        Args {
            port: String::new(),
            baud_rate: 115200,
            data_bits: DataBits::Eight,
            flow_control: FlowControl::None,
            parity: Parity::None,
            stop_bits: StopBits::One,
            timeout: Duration::from_millis(0),
            mode: String::from("stdout"),
            capacity: 64,
        }
    }
}
fn main() {
    println!("Hello, world!");
    let mut pargs = pico_args::Arguments::from_env();

    if pargs.contains(["-h", "--help"]) {
        print!("{}", HELP);
        std::process::exit(0);
    }

    let mut get_value = |keys| {
        pargs
            .opt_value_from_fn(keys, |s| {
                if true {
                    Ok(s.to_owned())
                } else {
                    // This line is just for helping type inference
                    Err(MissingArgument)
                }
            })
            .unwrap()
        // .value_from_fn(keys, |s| {
        //     if true {
        //         Ok(Some(s.to_owned()))
        //     } else {
        //         // This line is just for helping type inference
        //         Err(MissingArgument)
        //     }
        // })
        // .or_else(|err| match err {
        //     MissingOption(_) => Ok(None),
        //     err => Err(err),
        // })
        // .unwrap()
    };

    let dargs = Args::default();

    use pico_args::Error::*;
    let args = Args {
        port: get_value(["-p", "--port"]).unwrap_or_else(|| {
            let ports = serialport::available_ports().expect("No ports found!");
            if ports.len() == 1 {
                let mut ports = ports;
                ports.remove(0).port_name
            } else {
                // TODO: prettify the output of this section
                println!("Found ports:");
                for p in ports {
                    println!("{:?}", p);
                }
                exit(0)
            }
        }),
        baud_rate: get_value(["-b", "--baud-rate"])
            .map(|x| {
                x.parse()
                    .expect("Baud rate option requires a numerical value")
            })
            .unwrap_or(dargs.baud_rate),
        data_bits: get_value(["-d", "--data-bits"])
            .map(|x| match x.as_str() {
                "5" => DataBits::Five,
                "6" => DataBits::Six,
                "7" => DataBits::Seven,
                "8" => DataBits::Eight,
                _ => panic!("Data bits option passed an invalid value"),
            })
            .unwrap_or(dargs.data_bits),
        flow_control: get_value(["-F", "--flow-control"])
            .map(|n| match n.to_ascii_lowercase().as_str() {
                "hardware" | "hw" => FlowControl::Hardware,
                "software" | "sw" => FlowControl::Software,
                "none" => FlowControl::None,
                _ => panic!("Flow Control option passed an invalid value"),
            })
            .unwrap_or(dargs.flow_control),
        parity: get_value(["-P", "--parity"])
            .map(|n| match n.to_ascii_lowercase().as_str() {
                "none" => Parity::None,
                "odd" => Parity::Odd,
                "even" => Parity::Even,
                _ => panic!("Parity option passed an invalid value"),
            })
            .unwrap_or(dargs.parity),
        stop_bits: get_value(["-s", "--stop-bits"])
            .map(|n| match n.to_ascii_lowercase().as_str() {
                "1" | "one" => StopBits::One,
                "2" | "two" => StopBits::Two,
                _ => panic!("Stop Bits option passed an invalid value"),
            })
            .unwrap_or(dargs.stop_bits),
        timeout: get_value(["-T", "--timeout"])
            .map(|n| {
                n.parse()
                    .expect("Timeout option requires a numerical value")
            })
            .map(|t| Duration::from_millis(t))
            .unwrap_or(dargs.timeout),
        mode: get_value(["-w", "--mode"]).unwrap_or(dargs.mode),
        capacity: get_value(["-c", "--capacity"])
            .map(|x| {
                x.parse()
                    .expect("Capacity option requires a numerical value")
            })
            .unwrap_or(dargs.capacity),
        ..dargs
    };

    if pargs.contains(["-o", "--output-file"]) {
        todo!("File argument not supported yet")
    }
    if pargs.contains(["-f", "--format"]) {
        todo!("Parse format argument not supported yet")
    }

    let remaining = pargs.finish();
    if !remaining.is_empty() {
        eprintln!("Warning: unused arguments left: {:?}.", remaining);
    }

    let port = serialport::new(args.port.clone(), args.baud_rate)
        .data_bits(args.data_bits)
        .flow_control(args.flow_control)
        .parity(args.parity)
        .stop_bits(args.stop_bits)
        .timeout(args.timeout)
        .open()
        .expect("Could not open the serial port");

    if args.mode.eq_ignore_ascii_case("stdout") {
        serial_to_writer(port, stdout().lock(), false, &args)
    } else if args.mode.eq_ignore_ascii_case("iterm") {
        serial_iterm(port, &args)
    } else if args.mode.eq_ignore_ascii_case("lines") {
        serial_line_interactive(port, &args)
    } else if args.mode.eq_ignore_ascii_case("file") {
        serial_to_writer(port, File::open("output.txt").unwrap(), true, &args)
    }
    todo!("Implement the tui");
}

fn serial_line_interactive(port: Box<dyn SerialPort>, args: &Args) {
    todo!()
}

fn serial_iterm(port: Box<dyn SerialPort>, args: &Args) {
    todo!()
}

/// Read only streaming from the serial port
/// Writes the data to the Write object (buffered for performance)
fn serial_to_writer(mut port: Box<dyn SerialPort>, out: impl Write, counts: bool, args: &Args) {
    let mut out = BufWriter::with_capacity(args.capacity, out);

    loop {
        match io::copy(&mut port, &mut out) {
            Ok(n) => {
                if counts {
                    // TODO: count bytes, bytes/sec, lines, etc
                    println!("{}", n);
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::TimedOut => (),
            Err(e) => serial_read_error(&port, e),
        }
    }
}

fn serial_read_error(port: &Box<dyn SerialPort>, e: std::io::Error) {
    // TODO: better error messages
    eprintln!("{:?}", e);
    // TODO: Should exit/panic?
}
