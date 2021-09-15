//! Install tools and build the `esp-idf` using `platformio`.

use std::convert::TryFrom;
use std::{env, path::PathBuf};

use anyhow::*;

use embuild::build;
use embuild::kconfig;
use embuild::pio;
use embuild::pio::project;
use embuild::{bindgen, path_buf};

use super::EspIdfBuildOutput;

pub(crate) fn main() -> Result<EspIdfBuildOutput<impl Iterator<Item = (String, kconfig::Value)>>> {
    let (pio_scons_vars, link_args) =
        if let Some(pio_scons_vars) = project::SconsVariables::from_piofirst() {
            println!("cargo:info=PIO->Cargo build detected: generating bindings only");

            (pio_scons_vars, None)
        } else {
            let pio = pio::Pio::install_default()?;

            let resolution = pio::Resolver::new(pio.clone())
                .params(pio::ResolutionParams {
                    platform: Some("espressif32".into()),
                    frameworks: vec!["espidf".into()],
                    target: Some(env::var("TARGET")?),
                    ..Default::default()
                })
                .resolve(true)?;

            let mut builder =
                project::Builder::new(PathBuf::from(env::var("OUT_DIR")?).join("esp-idf"));

            builder
                .enable_scons_dump()
                .enable_c_entry_points()
                .options(build::env_options_iter("ESP_IDF_SYS_PIO_CONF")?)
                .files(build::tracked_globs_iter(path_buf!["."], &["patches/**"])?)
                .files(build::tracked_env_globs_iter("ESP_IDF_SYS_GLOB")?);

            for patch in super::STABLE_PATCHES {
                builder.platform_package_patch(PathBuf::from(patch), path_buf!["framework-espidf"]);
            }

            let project_path = builder.generate(&resolution)?;

            pio.build(&project_path, env::var("PROFILE")? == "release")?;

            let pio_scons_vars = project::SconsVariables::from_dump(&project_path)?;

            let link_args = build::LinkArgsBuilder::try_from(&pio_scons_vars)?.build()?;

            (pio_scons_vars, Some(link_args))
        };

    let sdkconfig = path_buf![
        &pio_scons_vars.project_dir,
        if pio_scons_vars.release_build {
            "sdkconfig.release"
        } else {
            "sdkconfig.debug"
        }
    ];

    let build_output = EspIdfBuildOutput {
        cincl_args: build::CInclArgs::try_from(&pio_scons_vars)?,
        link_args,
        bindgen: bindgen::Factory::from_scons_vars(&pio_scons_vars)?,
        kconfig_args: kconfig::try_from_config_file(sdkconfig.clone())
            .with_context(|| anyhow!("Failed to read '{:?}'", sdkconfig))?
            .map(|(key, value)| if key.starts_with("CONFIG_") { (key["CONFIG_".len()..].into(), value) } else { (key, value) }),
    };

    Ok(build_output)
}
