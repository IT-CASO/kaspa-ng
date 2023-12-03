use crate::imports::*;
use crate::mobile::MobileMenu;
use egui::load::Bytes;
use egui_notify::Toasts;
use kaspa_metrics::MetricsSnapshot;
use kaspa_wallet_core::api::TransactionDataGetResponse;
use kaspa_wallet_core::events::Events as CoreWallet;
use kaspa_wallet_core::storage::{Binding, Hint, PrvKeyDataInfo};
use std::borrow::Cow;
#[allow(unused_imports)]
use workflow_i18n::*;

pub enum Exception {
    UtxoIndexNotEnabled { url: Option<String> },
}

pub struct Core {
    is_shutdown_pending: bool,
    settings_storage_requested: bool,
    last_settings_storage_request: Instant,

    runtime: Runtime,
    wallet: Arc<dyn WalletApi>,
    application_events_channel: ApplicationEventsChannel,
    deactivation: Option<Module>,
    module: Module,
    stack: VecDeque<Module>,
    modules: HashMap<TypeId, Module>,
    pub settings: Settings,
    pub toasts: Toasts,
    screenshot: Option<Arc<ColorImage>>,
    pub mobile_style: egui::Style,
    pub default_style: egui::Style,

    state: State,
    hint: Option<Hint>,
    discard_hint: bool,
    exception: Option<Exception>,

    pub metrics: Option<Box<MetricsSnapshot>>,
    pub wallet_descriptor: Option<WalletDescriptor>,
    pub wallet_list: Vec<WalletDescriptor>,
    pub prv_key_data_map: HashMap<PrvKeyDataId, Arc<PrvKeyDataInfo>>,
    pub account_collection: Option<AccountCollection>,
    // pub selected_account: Option<Account>,
    pub release: Option<Release>,
}

impl Core {
    /// Core initialization
    pub fn new(
        cc: &eframe::CreationContext<'_>,
        runtime: crate::runtime::Runtime,
        mut settings: Settings,
    ) -> Self {
        crate::fonts::init_fonts(cc);
        egui_extras::install_image_loaders(&cc.egui_ctx);

        let mut default_style = (*cc.egui_ctx.style()).clone();

        default_style.text_styles.insert(
            TextStyle::Name("CompositeButtonSubtext".into()),
            FontId {
                size: 10.0,
                family: FontFamily::Proportional,
            },
        );

        let mut mobile_style = (*cc.egui_ctx.style()).clone();

        mobile_style.text_styles.insert(
            TextStyle::Name("CompositeButtonSubtext".into()),
            FontId {
                size: 12.0,
                family: FontFamily::Proportional,
            },
        );

        // println!("style: {:?}", style.text_styles);
        mobile_style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(22.0, egui::FontFamily::Proportional),
        );
        mobile_style.text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::new(18.0, egui::FontFamily::Proportional),
        );
        mobile_style.text_styles.insert(
            egui::TextStyle::Button,
            egui::FontId::new(18.0, egui::FontFamily::Proportional),
        );
        mobile_style.text_styles.insert(
            egui::TextStyle::Monospace,
            egui::FontId::new(18.0, egui::FontFamily::Proportional),
        );

        apply_theme_by_name(
            &cc.egui_ctx,
            settings.user_interface.theme_color.as_str(),
            settings.user_interface.theme_style.as_str(),
        );

        // cc.egui_ctx.set_style(style);

        // This is also where you can customize the look and feel of egui using
        // `cc.egui_ctx.set_visuals` and `cc.egui_ctx.set_fonts`.

        // Load previous app state (if any).
        // Note that you must enable the `persistence` feature for this to work.
        // if let Some(storage) = cc.storage {
        //     return eframe::get_value(storage, eframe::APP_KEY).unwrap_or_default();
        // }

        let modules: HashMap<TypeId, Module> = {
            cfg_if! {
                if #[cfg(not(target_arch = "wasm32"))] {
                    crate::modules::register_generic_modules(&runtime).into_iter().chain(
                        crate::modules::register_native_modules(&runtime)
                    ).collect()
                } else {
                    crate::modules::register_generic_modules(&runtime)
                }
            }
        };

        let mut module = modules
            .get(&TypeId::of::<modules::Overview>())
            .unwrap()
            .clone();

        if settings.version != env!("CARGO_PKG_VERSION") {
            settings.version = env!("CARGO_PKG_VERSION").to_string();
            settings.store_sync().unwrap();

            module = modules
                .get(&TypeId::of::<modules::Changelog>())
                .unwrap()
                .clone();
        }

        let application_events_channel = runtime.application_events().clone();
        let wallet = runtime.wallet().clone();

        let mut this = Self {
            runtime,
            is_shutdown_pending: false,
            settings_storage_requested: false,
            last_settings_storage_request: Instant::now(),

            wallet,
            application_events_channel,
            deactivation: None,
            module,
            modules: modules.clone(),
            stack: VecDeque::new(),
            settings: settings.clone(),
            toasts: Toasts::default(),
            screenshot: None,
            // status_bar_message: None,
            default_style,
            mobile_style,

            wallet_descriptor: None,
            wallet_list: Vec::new(),
            prv_key_data_map: HashMap::new(),
            account_collection: None,
            // selected_account: None,
            metrics: None,
            state: Default::default(),
            hint: None,
            discard_hint: false,
            exception: None,

            release: None,
        };

        modules.values().for_each(|module| {
            module.init(&mut this);
        });

        this.wallet_update_list();

        #[cfg(not(target_arch = "wasm32"))]
        spawn(async move {
            use workflow_core::task::sleep;

            loop {
                println!("Checking version...");
                let _ = check_version().await;
                sleep(Duration::from_secs(60 * 60 * 6)).await;
            }
        });

        this
    }

    pub fn select<T>(&mut self)
    where
        T: 'static,
    {
        self.select_with_type_id(TypeId::of::<T>());
    }

    pub fn select_with_type_id(&mut self, type_id: TypeId) {
        let module = self.modules.get(&type_id).expect("Unknown module");

        if self.module.type_id() != module.type_id() {
            let next = module.clone();
            self.stack.push_back(self.module.clone());
            self.deactivation = Some(self.module.clone());
            self.module = next.clone();
            next.activate(self);

            #[cfg(not(target_arch = "wasm32"))]
            {
                let type_id = self.module.type_id();
                crate::runtime::services::kaspa::update_logs_flag()
                    .store(type_id == TypeId::of::<modules::Logs>(), Ordering::Relaxed);
            }
        }
    }

    pub fn has_stack(&self) -> bool {
        !self.stack.is_empty()
    }

    pub fn back(&mut self) {
        if let Some(module) = self.stack.pop_back() {
            self.module = module;
        }
    }

    pub fn sender(&self) -> crate::runtime::channel::Sender<Events> {
        self.application_events_channel.sender.clone()
    }

    pub fn store_settings(&self) {
        self.application_events_channel
            .sender
            .try_send(Events::StoreSettings)
            .unwrap();
    }

    pub fn wallet(&self) -> &Arc<dyn WalletApi> {
        &self.wallet
    }

    pub fn state(&self) -> &State {
        &self.state
    }

    pub fn wallet_list(&self) -> &Vec<WalletDescriptor> {
        &self.wallet_list
    }

    pub fn account_collection(&self) -> &Option<AccountCollection> {
        &self.account_collection
    }

    pub fn modules(&self) -> &HashMap<TypeId, Module> {
        &self.modules
    }

    pub fn metrics(&self) -> &Option<Box<MetricsSnapshot>> {
        &self.metrics
    }

    pub fn module(&self) -> &Module {
        &self.module
    }

    pub fn get<T>(&self) -> Ref<'_, T>
    where
        T: ModuleT + 'static,
    {
        let cell = self.modules.get(&TypeId::of::<T>()).unwrap();
        Ref::map(cell.inner.module.borrow(), |r| {
            (r).as_any()
                .downcast_ref::<T>()
                .expect("unable to downcast section")
        })
    }

    pub fn get_mut<T>(&mut self) -> RefMut<'_, T>
    where
        T: ModuleT + 'static,
    {
        let cell = self.modules.get_mut(&TypeId::of::<T>()).unwrap();
        RefMut::map(cell.inner.module.borrow_mut(), |r| {
            (r).as_any_mut()
                .downcast_mut::<T>()
                .expect("unable to downcast_mut module")
        })
    }
}

impl eframe::App for Core {
    #[cfg(not(target_arch = "wasm32"))]
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.is_shutdown_pending = true;
        crate::runtime::halt();
        println!("{}", i18n("bye!"));
    }

    /// Called each time the UI needs repainting, which may be many times per second.
    /// Put your widgets into a `SidePanel`, `TopPanel`, `CentralPanel`, `Window` or `Area`.
    fn update(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        // println!("update...");
        for event in self.application_events_channel.iter() {
            if let Err(err) = self.handle_events(event.clone(), ctx, frame) {
                log_error!("error processing wallet runtime event: {}", err);
            }
        }

        if self.is_shutdown_pending {
            return;
        }

        if self.settings_storage_requested
            && self.last_settings_storage_request.elapsed() > Duration::from_secs(5)
        {
            self.settings_storage_requested = false;
            self.settings.store_sync().unwrap();
        }

        ctx.input(|input| {
            input.events.iter().for_each(|event| {
                if let Event::Key {
                    key,
                    pressed,
                    modifiers,
                    repeat,
                } = event
                {
                    self.handle_keyboard_events(*key, *pressed, modifiers, *repeat);
                }
            });

            for event in &input.raw.events {
                if let Event::Screenshot { image, .. } = event {
                    self.screenshot = Some(image.clone());
                }
            }
        });

        // - TODO - TOAST BACKGROUND
        // ---
        let current_visuals = ctx.style().visuals.clone(); //.widgets.noninteractive;
        let mut visuals = current_visuals.clone();
        visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(0, 0, 0);
        ctx.set_visuals(visuals);
        self.toasts.show(ctx);
        ctx.set_visuals(current_visuals);
        // ---

        // cfg_if! {
        //     if #[cfg(target_arch = "wasm32")] {
        //         visuals.interact_cursor = Some(CursorIcon::PointingHand);
        //     }
        // }

        if !self.settings.initialized {
            cfg_if! {
                if #[cfg(not(target_arch = "wasm32"))] {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        self.modules
                        .get(&TypeId::of::<modules::Welcome>())
                        .unwrap()
                        .clone()
                        .render(self, ctx, frame, ui);
                    });

                    return;
                }
            }
        }

        let device = runtime().device();

        if !self.module.modal() && !device.is_singular_layout() {
            egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
                Menu::new(self).render(ui);
                // self.render_menu(ui, frame);
            });
        }

        if device.is_singular_layout() {
            if !device.is_mobile() {
                egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
                    Status::new(self).render(ui);
                    egui::warn_if_debug_build(ui);
                });
            }

            let device_width = 390.0;
            let margin_width = (ctx.screen_rect().width() - device_width) * 0.5;

            SidePanel::right("portrait_right")
                .exact_width(margin_width)
                .resizable(false)
                .show_separator_line(true)
                .frame(Frame::default().fill(Color32::BLACK))
                .show(ctx, |_ui| {});
            SidePanel::left("portrait_left")
                .exact_width(margin_width)
                .resizable(false)
                .show_separator_line(true)
                .frame(Frame::default().fill(Color32::BLACK))
                .show(ctx, |_ui| {});

            CentralPanel::default()
                .frame(Frame::default().fill(ctx.style().visuals.panel_fill))
                .show(ctx, |ui| {
                    ui.set_max_width(device_width);

                    egui::TopBottomPanel::bottom("mobile_bottom_panel").show_inside(ui, |ui| {
                        Status::new(self).render(ui);
                    });

                    if device.is_mobile() {
                        egui::TopBottomPanel::bottom("mobile_menu_panel").show_inside(ui, |ui| {
                            MobileMenu::new(self).render(ui);
                        });
                    }

                    egui::CentralPanel::default()
                        .frame(
                            Frame::default()
                                .inner_margin(0.)
                                .outer_margin(4.)
                                .fill(ctx.style().visuals.panel_fill),
                        )
                        .show_inside(ui, |ui| {
                            self.module.clone().render(self, ctx, frame, ui);
                        });
                });
        } else {
            egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
                Status::new(self).render(ui);
                // self.render_status(ui);
                egui::warn_if_debug_build(ui);
            });

            egui::CentralPanel::default().show(ctx, |ui| {
                self.module.clone().render(self, ctx, frame, ui);
            });
        }

        // if false {
        //     egui::Window::new("Window").show(ctx, |ui| {
        //         ui.label("Windows can be moved by dragging them.");
        //         ui.label("They are automatically sized based on contents.");
        //         ui.label("You can turn on resizing and scrolling if you like.");
        //         ui.label("You would normally choose either panels OR windows.");
        //     });
        // }

        if let Some(module) = self.deactivation.take() {
            module.deactivate(self);
        }

        #[cfg(not(target_arch = "wasm32"))]
        if let Some(screenshot) = self.screenshot.as_ref() {
            match rfd::FileDialog::new().save_file() {
                Some(mut path) => {
                    path.set_extension("png");
                    let screen_rect = ctx.screen_rect();
                    let pixels_per_point = ctx.pixels_per_point();
                    let screenshot = screenshot.clone();
                    let sender = self.sender();
                    std::thread::Builder::new()
                        .name("screenshot".to_string())
                        .spawn(move || {
                            let image = screenshot.region(&screen_rect, Some(pixels_per_point));
                            image::save_buffer(
                                &path,
                                image.as_raw(),
                                image.width() as u32,
                                image.height() as u32,
                                image::ColorType::Rgba8,
                            )
                            .unwrap();

                            sender
                                .try_send(Events::Notify {
                                    user_notification: UserNotification::success(format!(
                                        "Capture saved to\n{}",
                                        path.to_string_lossy()
                                    )),
                                })
                                .unwrap()
                        })
                        .expect("Unable to spawn screenshot thread");
                    self.screenshot.take();
                }
                None => {
                    self.screenshot.take();
                }
            }
        }
    }
}

impl Core {
    fn _render_splash(&mut self, ui: &mut Ui) {
        let logo_rect = ui.ctx().screen_rect();
        let logo_size = logo_rect.size();
        Image::new(ImageSource::Bytes {
            uri: Cow::Borrowed("bytes://logo.svg"),
            bytes: Bytes::Static(crate::app::KASPA_NG_LOGO_SVG),
        })
        .maintain_aspect_ratio(true)
        // .max_size(logo_size)
        // .fit_to_fraction(vec2(0.9,0.8))
        .fit_to_exact_size(logo_size)
        // .fit_to_exact_size(logo_size)
        // .shrink_to_fit()
        // .bg_fill(Color32::DARK_GRAY)
        .texture_options(TextureOptions::LINEAR)
        // .tint(Color32::from_f32(0.9_f32))
        .paint_at(ui, logo_rect);
    }

    pub fn handle_events(
        &mut self,
        event: Events,
        _ctx: &egui::Context,
        _frame: &mut eframe::Frame,
    ) -> Result<()> {
        match event {
            Events::ThemeChange => {
                if let Some(account_collection) = self.account_collection.as_ref() {
                    account_collection
                        .iter()
                        .for_each(|account| account.update_theme());
                }
            }
            Events::VersionUpdate(release) => {
                self.release = Some(release);
            }
            Events::StoreSettings => {
                self.settings_storage_requested = true;
                self.last_settings_storage_request = Instant::now();
            }
            Events::UpdateLogs => {}
            Events::Metrics { snapshot } => {
                self.metrics = Some(snapshot);
            }
            Events::Exit => {
                cfg_if! {
                    if #[cfg(not(target_arch = "wasm32"))] {
                        self.is_shutdown_pending = true;
                        _ctx.send_viewport_cmd(ViewportCommand::Close);
                    }
                }
            }
            Events::Error(error) => {
                runtime().notify(UserNotification::error(error.as_str()));
            }
            Events::WalletList { wallet_list } => {
                self.wallet_list = (*wallet_list).clone();
                self.wallet_list.sort();
            }
            Events::Notify {
                user_notification: notification,
            } => {
                notification.render(&mut self.toasts);
            }
            Events::Close { .. } => {}
            Events::UnlockSuccess => {}
            Events::UnlockFailure { .. } => {}
            Events::PrvKeyDataInfo {
                prv_key_data_info_map,
            } => {
                self.prv_key_data_map = prv_key_data_info_map;
            }
            Events::Wallet { event } => {
                match *event {
                    CoreWallet::UtxoProcStart => {}
                    CoreWallet::UtxoProcStop => {}
                    CoreWallet::UtxoProcError { message: _ } => {
                        // terrorln!(this,"{err}");
                    }
                    #[allow(unused_variables)]
                    CoreWallet::Connect { url, network_id } => {
                        // log_info!("Connected to {url:?} on network {network_id}");
                        self.state.is_connected = true;
                        self.state.url = url;
                        self.state.network_id = Some(network_id);
                    }
                    #[allow(unused_variables)]
                    CoreWallet::Disconnect {
                        url: _,
                        network_id: _,
                    } => {
                        self.state.is_connected = false;
                        self.state.sync_state = None;
                        self.state.is_synced = None;
                        self.state.server_version = None;
                        self.state.url = None;
                        self.state.network_id = None;
                        self.state.current_daa_score = None;
                        self.metrics = Some(Box::default());
                    }
                    CoreWallet::UtxoIndexNotEnabled { url } => {
                        self.exception = Some(Exception::UtxoIndexNotEnabled { url });
                    }
                    CoreWallet::SyncState { sync_state } => {
                        self.state.sync_state = Some(sync_state);
                    }
                    CoreWallet::ServerStatus {
                        is_synced,
                        server_version,
                        url,
                        network_id,
                    } => {
                        self.state.is_synced = Some(is_synced);
                        self.state.server_version = Some(server_version);
                        self.state.url = url;
                        self.state.network_id = Some(network_id);
                    }
                    CoreWallet::WalletHint { hint } => {
                        self.hint = hint;
                        self.discard_hint = false;
                    }
                    CoreWallet::WalletOpen {
                        wallet_descriptor,
                        account_descriptors,
                    }
                    | CoreWallet::WalletReload {
                        wallet_descriptor,
                        account_descriptors,
                    } => {
                        self.state.is_open = true;

                        self.wallet_descriptor = wallet_descriptor;
                        // let network_id = self.state.network_id.ok_or(Error::WalletOpenNetworkId)?;
                        let network_id = self
                            .state
                            .network_id
                            .unwrap_or(self.settings.node.network.into());
                        let account_descriptors =
                            account_descriptors.ok_or(Error::WalletOpenAccountDescriptors)?;
                        self.load_accounts(network_id, account_descriptors)?;
                        // self.update_account_list();
                    }
                    CoreWallet::AccountActivation { ids: _ } => {}
                    CoreWallet::AccountCreation { descriptor: _ } => {}
                    CoreWallet::AccountUpdate { descriptor } => {
                        let account_id = descriptor.account_id();
                        if let Some(account_collection) = self.account_collection.as_ref() {
                            if let Some(account) = account_collection.get(account_id) {
                                account.update(descriptor);
                            }
                        }
                    }
                    CoreWallet::WalletError { message: _ } => {}
                    CoreWallet::WalletClose => {
                        self.hint = None;
                        self.state.is_open = false;
                        self.account_collection = None;
                        self.wallet_descriptor = None;

                        self.modules.clone().into_iter().for_each(|(_, module)| {
                            module.reset(self);
                        });
                    }
                    CoreWallet::AccountSelection { id: _ } => {}
                    CoreWallet::DAAScoreChange { current_daa_score } => {
                        self.state.current_daa_score.replace(current_daa_score);
                    }
                    // Ignore scan notifications
                    CoreWallet::Scan { record: _ } => {}
                    // Ignore stasis notifications
                    CoreWallet::Stasis { record: _ } => {}
                    // This notification is for a UTXO change, which is
                    // a part of the Outgoing transaction, we ignore it.
                    CoreWallet::Change { record: _ } => {}
                    // A transaction has been confirmed
                    CoreWallet::Maturity { record } => match record.binding().clone() {
                        Binding::Account(id) => {
                            self.account_collection
                                .as_ref()
                                .and_then(|account_collection| {
                                    account_collection.get(&id).map(|account| {
                                        account.transactions().replace_or_insert(
                                            Transaction::new_confirmed(Arc::new(record)),
                                        );
                                    })
                                });
                        }
                        Binding::Custom(_) => {
                            panic!("custom binding not supported");
                        }
                    },
                    // Observing a new, unconfirmed transaction
                    CoreWallet::External { record }
                    | CoreWallet::Outgoing { record }
                    | CoreWallet::Pending { record } => match record.binding().clone() {
                        Binding::Account(id) => {
                            self.account_collection
                                .as_ref()
                                .and_then(|account_collection| {
                                    account_collection.get(&id).map(|account| {
                                        account.transactions().replace_or_insert(
                                            Transaction::new_processing(Arc::new(record)),
                                        );
                                    })
                                });
                        }
                        Binding::Custom(_) => {
                            panic!("custom binding not supported");
                        }
                    },

                    CoreWallet::Reorg { record } => match record.binding().clone() {
                        Binding::Account(id) => {
                            self.account_collection
                                .as_mut()
                                .and_then(|account_collection| {
                                    account_collection
                                        .get(&id)
                                        .map(|account| account.transactions().remove(record.id()))
                                });
                        }
                        Binding::Custom(_) => {
                            panic!("custom binding not supported");
                        }
                    },

                    CoreWallet::Balance { balance, id } => {
                        if let Some(account_collection) = &self.account_collection {
                            if let Some(account) = account_collection.get(&id.into()) {
                                // println!("*** updating account balance: {}", id);
                                account.update_balance(balance)?;
                            } else {
                                log_error!("unable to find account {}", id);
                            }
                        } else {
                            log_error!(
                                "received CoreWallet::Balance while account collection is empty"
                            );
                        }
                    }
                }
            }
        }

        Ok(())
    }

    pub fn wallet_update_list(&self) {
        let runtime = self.runtime.clone();
        spawn(async move {
            let wallet_list = runtime.wallet().wallet_enumerate().await?;
            runtime
                .send(Events::WalletList {
                    wallet_list: Arc::new(wallet_list),
                })
                .await?;
            Ok(())
        });
    }

    fn load_accounts(
        &mut self,
        network_id: NetworkId,
        account_descriptors: Vec<AccountDescriptor>,
    ) -> Result<()> {
        let application_events_sender = self.application_events_channel.sender.clone();

        let account_list = account_descriptors
            .into_iter()
            .map(Account::from)
            .collect::<Vec<_>>();

        self.account_collection = Some(account_list.clone().into());

        let runtime = self.runtime.clone();
        spawn(async move {
            let prv_key_data_info_map = runtime
                .wallet()
                .prv_key_data_enumerate()
                .await?
                .clone()
                .into_iter()
                .map(|prv_key_data_info| (*prv_key_data_info.id(), prv_key_data_info))
                .collect::<HashMap<_, _>>();
            application_events_sender
                .send(Events::PrvKeyDataInfo {
                    prv_key_data_info_map,
                })
                .await?;

            let account_ids = account_list
                .iter()
                .map(|account| account.id())
                .collect::<Vec<_>>();
            let account_map: HashMap<AccountId, Account> = account_list
                .clone()
                .into_iter()
                .map(|account| (account.id(), account))
                .collect::<HashMap<_, _>>();

            let futures = account_ids
                .into_iter()
                .map(|account_id| {
                    runtime
                        .wallet()
                        .transaction_data_get_range(account_id, network_id, 0..128)
                })
                .collect::<Vec<_>>();

            let transaction_data = join_all(futures)
                .await
                .into_iter()
                .map(|v| v.map_err(Error::from))
                .collect::<Result<Vec<_>>>()?;

            transaction_data.into_iter().for_each(|data| {
                let TransactionDataGetResponse {
                    account_id,
                    transactions,
                    start: _,
                    total,
                } = data;

                if let Some(account) = account_map.get(&account_id) {
                    if let Err(err) = account.load_transactions(transactions, total) {
                        log_error!("error loading transactions into account {account_id}: {err}");
                    }
                } else {
                    log_error!("unable to find account {}", account_id);
                }
            });

            runtime.wallet().accounts_activate(None).await?;

            Ok(())
        });

        Ok(())
    }

    fn handle_keyboard_events(
        &mut self,
        key: Key,
        pressed: bool,
        modifiers: &Modifiers,
        _repeat: bool,
    ) {
        if !pressed {
            return;
        }

        if modifiers.ctrl || modifiers.mac_cmd {
            match key {
                Key::O => {
                    self.select::<modules::WalletOpen>();
                }
                Key::N => {
                    self.select::<modules::WalletCreate>();
                }
                Key::M => {
                    runtime().device().toggle_mobile();
                }
                Key::P => {
                    runtime().device().toggle_portrait();
                }
                _ => {}
            }
        }

        if (modifiers.ctrl || modifiers.mac_cmd) && modifiers.shift {
            match key {
                Key::T => {
                    self.select::<modules::Testing>();
                }
                Key::M => {
                    runtime().device().toggle_mobile();
                }
                Key::P => {
                    runtime().device().toggle_portrait();
                }
                _ => {}
            }
        }
    }

    pub fn apply_mobile_style(&self, ui: &mut Ui) {
        ui.style_mut().text_styles = self.mobile_style.text_styles.clone();
    }

    pub fn apply_default_style(&self, ui: &mut Ui) {
        ui.style_mut().text_styles = self.default_style.text_styles.clone();
    }
}
