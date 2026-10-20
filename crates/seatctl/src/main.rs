use clap::{Parser, Subcommand};
use seat_udev::assign::{Assigner, AssignmentGroup, RealAssignmentBackend};
use seat_udev::{Context, Filter, SeatName};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

#[derive(Debug, Parser)]
#[command(version, about = "Inspect and persist udev seat assignments")]
struct Args {
    #[arg(long, default_value_t = 15)]
    timeout_seconds: u64,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Assign a complete ID_FOR_SEAT device group to a seat.
    Add { device: PathBuf, seat: SeatName },
    /// Remove the persistent assignment for a device group.
    Remove { device: PathBuf },
    /// Remove every managed persistent assignment.
    Flush,
    /// Show current seats/devices and persisted assignment rules.
    Status,
}

fn main() -> ExitCode {
    match run(Args::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("seatctl: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Args) -> Result<(), Box<dyn std::error::Error>> {
    let assigner = Assigner::new(
        RealAssignmentBackend::system(),
        Duration::from_secs(args.timeout_seconds),
    );
    match args.command {
        Command::Add { device, seat } => {
            let group = assigner.assign(&device, &seat)?;
            print_group("assigned", &group);
            if !group.has_master {
                eprintln!(
                    "warning: group {} has no master-of-seat device; it cannot create {} alone",
                    group.stable_id, seat
                );
            }
        }
        Command::Remove { device } => {
            print_group("unassigned", &assigner.unassign(&device)?);
        }
        Command::Flush => {
            for group in assigner.flush()? {
                print_group("unassigned", &group);
            }
        }
        Command::Status => {
            let context = Context::new()?;
            println!("seats:");
            for seat in context.seats() {
                println!("  {seat}");
                for device in context.devices(&seat, Filter::All) {
                    println!(
                        "    {}{}{}",
                        device
                            .devnode
                            .as_deref()
                            .unwrap_or(&device.syspath)
                            .display(),
                        if device.master_of_seat {
                            " [master]"
                        } else {
                            ""
                        },
                        if device.uaccess { " [uaccess]" } else { "" }
                    );
                }
            }
            println!("persistent assignments:");
            for assignment in assigner.persisted()? {
                println!(
                    "  {} -> {} ({})",
                    assignment.stable_id,
                    assignment.seat,
                    assignment.path.display()
                );
            }
        }
    }
    Ok(())
}

fn print_group(action: &str, group: &AssignmentGroup) {
    println!("{action} group {}:", group.stable_id);
    for device in &group.devices {
        println!("  {}", device.display());
    }
}
