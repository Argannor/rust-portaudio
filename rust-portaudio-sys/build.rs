// The MIT License (MIT)
//
// Copyright (c) 2013 Jeremy Letang (letang.jeremy@gmail.com)
//
// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of
// the Software, and to permit persons to whom the Software is furnished to do so,
// subject to the following conditions:
//
// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.
//
// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS
// FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
// COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
// IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
// CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

extern crate pkg_config;

use std::env;
use std::fmt::Display;
use std::path::Path;
use std::process::Command;

#[cfg(all(unix, not(target_os = "linux")))]
use unix_platform as platform;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");

    println!("cargo:rerun-if-env-changed=PORTAUDIO_ONLY_STATIC");
    if env::var("PORTAUDIO_ONLY_STATIC").is_err() {
        // If pkg-config finds a library on the system, we are done
        if pkg_config::Config::new().atleast_version("19").find("portaudio-2.0").is_ok() {
            return;
        }
    }

    build();
}

fn build() {
    // retrieve cargo deps out dir
    let out_dir_str = env::var("OUT_DIR").unwrap();
    let out_dir = Path::new(&out_dir_str);

    let static_lib = out_dir.join("lib/libportaudio.a");
    if let Err(_) = ::std::fs::metadata(static_lib) {
        platform::download();
        platform::build(out_dir);
    }

    platform::print_libs(out_dir);
}

// Similar to unwrap, but panics on just the error value
#[allow(dead_code)]
fn err_to_panic<T, E: Display>(result: Result<T, E>) -> T {
    match result {
        Ok(x) => x,
        Err(e) => panic!("{}", e)
    }
}

fn run(command: &mut Command) {
    let string = format!("{:?}", command);
    let status = err_to_panic(command.status());
    if !status.success() {
        panic!("`{}` did not execute successfully", string);
    }
}

#[allow(dead_code)]
mod unix_platform {
    use std::process::Command;
    use std::path::Path;

    use std::env;

    use super::{err_to_panic, run};

    pub const PORTAUDIO_URL: &'static str = "http://www.portaudio.com/archives/pa_stable_v19_20140130.tgz";
    pub const PORTAUDIO_TAR: &'static str = "pa_stable_v19_20140130.tgz";
    pub const PORTAUDIO_FOLDER: &'static str = "portaudio";

    pub fn download() {
        run(Command::new("curl").arg(PORTAUDIO_URL).arg("-O"));
    }

    pub fn build(out_dir: &Path) {
        // untar portaudio sources
        run(Command::new("tar").arg("xvf").arg(PORTAUDIO_TAR));

        // change dir to the portaudio folder
        err_to_panic(env::set_current_dir(PORTAUDIO_FOLDER));

        // run portaudio autoconf
        let mut configure = Command::new("./configure");
        configure.args(&["--disable-shared", "--enable-static"]); // Only build static lib
        configure.args(&["--prefix", out_dir.to_str().unwrap()]); // Install on the outdir
        configure.arg("--with-pic"); // Build position-independent code (required by Rust)

        // cross platform builds
        let rustc_linker = env::var("RUSTC_LINKER"); // if the linker is configured to be non default it will look like this /usr/bin/arm-linux-gnueabihf-gcc
        let cross_platform_args = match rustc_linker {
            Ok(linker_path) => {
                let linker_name = linker_path.split('/').last().unwrap();
                let last_dash_index = linker_name.rfind('-').unwrap();
                let target_name: String = linker_name.chars().take(last_dash_index).collect();
                // arm-linux-gnueabihf
                vec![format!("--target={target_name}"), format!("--host={target_name}")]
            },
            Err(_) => vec![]
        };
        configure.args(&cross_platform_args);

        run(&mut configure);

        // then make
        run(&mut Command::new("make"));

        // "install" on the outdir
        run(Command::new("make").arg("install"));

        // return to rust-portaudio root
        err_to_panic(env::set_current_dir(".."));

        // cleaning portaudio sources
        run(Command::new("rm").arg("-rf")
            .args(&[PORTAUDIO_TAR, PORTAUDIO_FOLDER]));
    }

    pub fn print_libs(out_dir: &Path) {
        let out_str = out_dir.to_str().unwrap();
        println!("cargo:rustc-flags=-L native={}/lib -l static=portaudio", out_str);
    }
}

#[cfg(target_os = "linux")]
mod platform {
    use pkg_config;
    use std::process::Command;
    use super::unix_platform;
    use std::path::Path;

    use super::{run, err_to_panic};

    pub fn download() {
        run(Command::new("wget").arg(unix_platform::PORTAUDIO_URL));
    }

    pub fn build(out_dir: &Path) {
        unix_platform::build(out_dir);
    }

    pub fn print_libs(out_dir: &Path) {
        let portaudio_pc_file = out_dir.join("lib/pkgconfig/portaudio-2.0.pc");
        let portaudio_pc_file = portaudio_pc_file.to_str().unwrap();

        err_to_panic(pkg_config::Config::new().statik(true).find(portaudio_pc_file));
    }
}

#[cfg(windows)]
mod platform {
    use std;
    use std::path::Path;
    use std::process::Command;

    extern crate cmake;

    pub fn download() {
        let mut command = Command::new("cmake");

        command.arg("-P");
        command.arg("download.cmake");

        match command.status() {
            Ok(status) =>
                if !status.success() {
                    panic!("Failed to execute command: {:?}", command)
                },
            Err(error) =>
                panic!("Failed to execute command: {:?}\n{}", command, error)
        }
    }

    pub fn build(out_dir: &Path) {
        let source_path = out_dir.join("portaudio");
        // Note: the 'PA_WDMKS_NO_KSGUID_LIB' preprocessor definition is a
        // workaround for an issue which is fixed in the newer versions. See
        // https://app.assembla.com/spaces/portaudio/subversion/commits/1944
        cmake::Config::new(source_path)
            // .generator("MSYS Makefiles")
            .define("CMAKE_ARCHIVE_OUTPUT_DIRECTORY_DEBUG", out_dir)
            .define("CMAKE_ARCHIVE_OUTPUT_DIRECTORY_RELEASE", out_dir)
            .cflag("-DPA_WDMKS_NO_KSGUID_LIB")
            .out_dir(out_dir)
            .build_target("portaudio_static")
            .build();
        std::fs::rename(
            out_dir.join(platform_specific_library_name()),
            out_dir.join("portaudio.lib")).unwrap();
    }

    pub fn print_libs(out_dir: &Path) {
        println!(
            "cargo:rustc-link-search=native={}", out_dir.to_str().unwrap());
    }

    #[cfg(target_arch = "x86")]
    fn platform_specific_library_name() -> &'static str {
        "portaudio_static_x86.lib"
    }

    #[cfg(target_arch = "x86_64")]
    fn platform_specific_library_name() -> &'static str {
        "portaudio_static_x64.lib"
    }
}
