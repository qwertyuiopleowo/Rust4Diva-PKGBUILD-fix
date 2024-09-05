use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};

use filenamify::filenamify;

use crate::config::write_config;
use crate::diva::{get_diva_folder, open_error_window};
use crate::modpacks::{self, ModPack, ModPackMod};
use crate::slint_generatedApp::App;
use crate::{FirstSetup, Loadout, SetupLogic, DIVA_CFG};
use rfd::AsyncFileDialog;
use serde::{Deserialize, Serialize};
use slint::private_unstable_api::re_exports::ColorScheme;
use slint::{Model, ModelRc, VecModel};
use slint_interpreter::ComponentHandle;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DmmConfig {
    #[serde(rename(serialize = "CurrentGame", deserialize = "CurrentGame"))]
    pub current_game: String,
    #[serde(rename(serialize = "Configs", deserialize = "Configs"))]
    pub configs: HashMap<String, DmmPDMMConfig>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DmmPDMMConfig {
    #[serde(rename(serialize = "Launcher", deserialize = "Launcher"), default)]
    pub launcher: Option<String>,
    #[serde(rename(serialize = "GamePath", deserialize = "GamePath"), default)]
    pub game_path: Option<String>,
    #[serde(rename(serialize = "ModsFolder", deserialize = "ModsFolder"), default)]
    pub mods_folder: Option<String>,
    #[serde(
        rename(serialize = "CurrentLoadout", deserialize = "CurrentLoadout"),
        default
    )]
    pub current_loadout: Option<String>,
    #[serde(rename(serialize = "Loadouts", deserialize = "Loadouts"), default)]
    pub loadouts: HashMap<String, Vec<DmmLoadoutMod>>,
}
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct DmmLoadoutMod {
    pub name: String,
    pub enabled: bool,
}

impl DmmLoadoutMod {
    pub fn to_packmod(self: &Self, mut mods_dir: PathBuf) -> ModPackMod {
        // let mut buf = PathBuf::from(mods_dir.clone());
        mods_dir.push(self.name.clone());
        ModPackMod {
            name: self.name.clone(),
            enabled: self.enabled.clone(),
            path: mods_dir.display().to_string(),
        }
    }
}

pub static DMM_CFG: LazyLock<Mutex<Option<DmmConfig>>> = LazyLock::new(|| Mutex::new(None));

pub async fn init(_diva_ui: &App) -> Result<(), slint::PlatformError> {
    let diva_dir = get_diva_folder();
    if let Ok(cfg) = DIVA_CFG.lock() {
        if cfg.first_run {
            let setup = FirstSetup::new()?;
            if cfg.dark_mode {
                setup.invoke_set_color_scheme(ColorScheme::Dark);
            }
            if !cfg.dark_mode {
                setup.invoke_set_color_scheme(ColorScheme::Light);
            }
            if let Some(diva_dir) = diva_dir {
                setup.set_diva_dir(diva_dir.into());
            }
            let import_handle = setup.as_weak();
            setup.global::<SetupLogic>().on_import_dmm(move || {
                let import_handle = import_handle.clone();
                let picker = AsyncFileDialog::new();
                tokio::spawn(async move {
                    if let Some(dmm_dir) = picker.pick_folder().await {
                        let mut buf = PathBuf::from(dmm_dir.path());
                        buf.push("Config.json");
                        if buf.exists() {
                            if let Ok(cfgstr) = fs::read_to_string(buf) {
                                match sonic_rs::from_str::<DmmConfig>(cfgstr.as_str()) {
                                    Ok(cfg) => {
                                        if let Ok(mut dmmcfg) = DMM_CFG.try_lock() {
                                            *dmmcfg = Some(cfg.clone());
                                        }
                                        if let Some(pdmm) =
                                            cfg.configs.get(&"Project DIVA Mega Mix+".to_string())
                                        {
                                            if let Some(mods_dir) = pdmm.mods_folder.clone() {
                                                println!("{}", mods_dir);
                                                let mut mbuf = PathBuf::from(mods_dir);
                                                mbuf.pop();
                                                if mbuf.exists() {
                                                    let _ = import_handle.upgrade_in_event_loop(
                                                        move |ui| {
                                                            ui.set_diva_dir(
                                                                mbuf.display().to_string().into(),
                                                            );
                                                        },
                                                    );
                                                }
                                            }
                                            let mut loadouts: Vec<Loadout> = Default::default();
                                            for (loadout, _mods) in pdmm.loadouts.iter() {
                                                println!("Loadout found: {}", loadout);
                                                loadouts.push(Loadout {
                                                    name: filenamify(loadout.clone()).into(),
                                                    import: true,
                                                });
                                            }
                                            loadouts.sort_by_key(|l| l.name.to_string());
                                            let _ =
                                                import_handle.upgrade_in_event_loop(move |ui| {
                                                    ui.set_loadouts(ModelRc::new(VecModel::from(
                                                        loadouts,
                                                    )));
                                                });
                                        }
                                    }
                                    Err(e) => {
                                        open_error_window(e.to_string());
                                    }
                                }
                            }
                        }
                    }
                });
            });

            let pdx_handle = setup.as_weak();
            setup
                .global::<SetupLogic>()
                .on_open_diva_picker(move |default_dir| {
                    let pdx_handle = pdx_handle.clone();
                    let picker = AsyncFileDialog::new().set_directory(default_dir.to_string());
                    tokio::spawn(async move {
                        match picker.pick_folder().await {
                            Some(pdx_dir) => {
                                let path = pdx_dir.path().display().to_string();
                                let mut buf = PathBuf::from(pdx_dir.path());
                                buf.push("DivaMegaMix.exe");
                                if buf.exists() {
                                    let _ = pdx_handle.upgrade_in_event_loop(move |ui| {
                                        ui.set_diva_dir(path.into());
                                    });
                                } else {
                                    open_error_window(
                                        "Selected Folder Does not contain DivaMegaMix.exe"
                                            .to_string(),
                                    );
                                }
                            }
                            None => {}
                        }
                    });
                });

            let apply_handle = setup.as_weak();
            setup.global::<SetupLogic>().on_apply(move || {
                let save_handle = apply_handle.clone();
                let ui = apply_handle.upgrade().unwrap();
                let mut diva_buf = PathBuf::from(ui.get_diva_dir().to_string());
                // do checks to make sure entries are valid
                if diva_buf.exists() && diva_buf.is_dir() {
                    diva_buf.push("DivaMegaMix.exe");
                    if !diva_buf.exists() {
                        open_error_window(
                            "Selected Directory does not contain DivaMegaMix.exe".to_string(),
                        );
                        return;
                    }
                    diva_buf.pop();
                } else {
                    open_error_window(
                        "Selected Project Diva directory does not exist or is a file".to_string(),
                    );
                    return;
                }
                let dark_mode = ui.get_dark_mode();
                println!("Dark Mode: {}", dark_mode);
                println!("PDMM+: {}", diva_buf.display());
                let mut prio = Vec::new();
                if let Ok(dmm_cfg_opt) = DMM_CFG.try_lock() {
                    if dmm_cfg_opt.is_some() {
                        let mut loadouts: Vec<ModPack> = Vec::new();
                        let dmm_cfg = dmm_cfg_opt.as_ref().unwrap();
                        match dmm_cfg.configs.get(&"Project DIVA Mega Mix+".to_string()) {
                            Some(config) => {
                                // if let Some(cl) = &config.current_loadout {
                                //     // currentload = Some(cl.clone());
                                // }

                                match ui
                                    .get_loadouts()
                                    .as_any()
                                    .downcast_ref::<VecModel<Loadout>>()
                                {
                                    Some(loadouts_mod) => {
                                        // let dmm_cfg
                                        for loadout in loadouts_mod.iter() {
                                            if loadout.import {
                                                let mut pack = ModPack::new(filenamify(
                                                    loadout.name.to_string(),
                                                ));

                                                println!(
                                                    r#"Converting Loadout: "{}" to modpack"#,
                                                    pack.name
                                                );
                                                for module in config
                                                    .loadouts
                                                    .get(&loadout.name.to_string())
                                                    .unwrap()
                                                {
                                                    if module.enabled {
                                                        pack.mods.push(
                                                            module.to_packmod(diva_buf.clone()),
                                                        )
                                                    }
                                                }
                                                loadouts.push(pack.clone());
                                                if config.current_loadout.is_some()
                                                    && filenamify(loadout.name.to_string())
                                                        == filenamify(
                                                            config
                                                                .current_loadout
                                                                .clone()
                                                                .unwrap()
                                                                .clone(),
                                                        )
                                                {
                                                    if let Some(l) = config
                                                        .loadouts
                                                        .get(&loadout.name.to_string())
                                                    {
                                                        for m in l {
                                                            prio.push(m.name.clone());
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    None => {}
                                }
                            }
                            None => {}
                        }

                        tokio::spawn(async move {
                            for pack in loadouts {
                                if let Err(e) = modpacks::save_modpack(pack).await {
                                    open_error_window(e.to_string());
                                }
                            }
                        });
                    }
                }

                if let Ok(mut cfg) = DIVA_CFG.try_lock() {
                    // let mut cfg = cfg.clone();
                    cfg.dark_mode = dark_mode;
                    cfg.diva_dir = diva_buf.display().to_string();
                    cfg.first_run = false;
                    if !prio.is_empty() {
                        println!("{:?}", prio);
                        cfg.priority = prio.clone();
                    }
                    let cfg = cfg.clone();
                    tokio::spawn(async move {
                        if let Err(e) = write_config(cfg.clone()).await {
                            open_error_window(e.to_string());
                        } else {
                            println!("Setup complete");
                            let _ = save_handle.upgrade_in_event_loop(|ui| {
                                ui.hide().unwrap();
                            });
                        }
                    });
                } else {
                    open_error_window("Unable to lock config".to_string());
                    return;
                }
            });
            setup.show()?;
        }
    }

    Ok(())
}
