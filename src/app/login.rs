use std::sync::Arc;

use tokio::time::{Duration, sleep};

use super::{App, send_event};
use crate::{
    event::{AppEvent, AuthEvent, NavigationEvent, PlaybackEvent},
    state::{LoginField, LoginMethod, Page, avatar},
};

impl App {
    pub(super) fn handle_login(&mut self) {
        let login = &mut self.state.login;
        login.loading = true;
        login.error = None;
        login.notice = None;

        let service = self.service.clone();
        let sender = self.state.events.sender();

        tokio::spawn(async move {
            match service.login_qr_create().await {
                Ok((url, key)) => {
                    send_event(&sender, AuthEvent::QRCreated { url, key }.into());
                }
                Err(e) => {
                    send_event(&sender, AuthEvent::Error(e.to_string()).into());
                }
            }
        });
    }

    /// After the session is gone: forget the user, drop the liked set that belonged to
    /// them, and go back to the login page.
    pub(super) fn handle_logout_done(&mut self) {
        self.state.navigation.user = None;
        // The face belonged to the session that just ended; a load still on its way is
        // dropped when it lands rather than put back on the bar.
        avatar::clear();
        self.state.login.loading = false;
        self.state.login.error = None;
        if let Ok(mut liked) = self.liked_ids.lock() {
            liked.clear();
        }
        self.playback.state.liked = false;
        self.toast("已退出登录".to_string());
        self.state
            .events
            .send(NavigationEvent::Navigate(Page::Login));
    }

    pub(super) fn handle_login_success(&mut self, info: ncm_api::LoginInfo) {
        self.toast(format!("登录成功: {}", info.nickname));
        let uid = info.uid;
        // The portrait is one more thing that arrives when it arrives, and it belongs to the
        // session that is starting here, so the URL is read before the login moves into the
        // state.
        let avatar_url = info.avatar_url.clone();
        self.state.login.loading = false;
        self.state.login.error = None;
        self.state.login.notice = None;
        self.state.navigation.user = Some(info);
        self.load_avatar(avatar_url);
        self.service.client().flush_cookies();
        if matches!(self.state.navigation.page, Page::Login | Page::Splash) {
            self.navigate_to_main();
        } else {
            // A login can land while the user is anywhere: the password and SMS forms are on
            // the login page, but the QR task keeps polling in the background. Every page draws
            // differently once there is a user, and the tab that is open fetched its content
            // without one — re-selecting it is what reloads that content under the new session.
            if let Some(api) = self.state.navigation.nav.selected_api() {
                send_event(
                    &self.state.events.sender(),
                    NavigationEvent::NavSelect(api.to_string()).into(),
                );
            }
            // The frame is stale in a way no other event covers (the topbar and the player bar
            // draw the session), so ask for one.
            send_event(&self.state.events.sender(), AppEvent::Repaint.into());
        }

        // After login, fetch the "我喜欢的音乐" list from the cloud so the local set and the playerbar icon stay in sync.
        let service = self.service.clone();
        let liked_ids = Arc::clone(&self.liked_ids);
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            match service.load_liked_song_ids(uid).await {
                Ok(ids) => {
                    if let Ok(mut guard) = liked_ids.lock() {
                        *guard = ids;
                    }
                    send_event(&sender, PlaybackEvent::LikedUpdated.into());
                }
                Err(e) => log::warn!("Failed to load liked song ids: {e}"),
            }
        });
    }

    /// Start loading the signed-in user's own portrait for the topbar, off the event loop like
    /// every other load. A URL that 403s, a timeout, bytes that are not an image: the portrait
    /// stays absent and the bar keeps the shape it has — there is no message to show for a
    /// decoration and nothing to fall back to.
    fn load_avatar(&mut self, url: String) {
        let session = avatar::session();
        let http = self.cover_http.clone();
        let picker = self.picker.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            if avatar::fetch_and_install(&http, &url, &picker, session).await {
                // The frame on screen was drawn without a portrait; only a wake-up gets it
                // drawn again.
                send_event(&sender, AppEvent::Repaint.into());
            }
        });
    }

    pub(super) fn handle_login_error(&mut self, e: String) {
        self.toast(format!("登录失败: {}", e));
        self.state.login.loading = false;
        self.state.login.error = Some(e);
        self.state.login.notice = None;
        if self.state.navigation.page == Page::Splash {
            self.navigate_to_main();
        }
    }

    /// `Enter` on one of the page's forms. The values are read here, once, so a request is
    /// built from the form as it was submitted rather than from whatever it becomes while the
    /// request is in flight.
    pub(super) fn handle_login_submit(&mut self, method: LoginMethod) {
        match method {
            LoginMethod::Qr => self.handle_login(),
            LoginMethod::Password => self.login_password(),
            LoginMethod::Sms => {
                if self.state.login.focus == LoginField::Phone {
                    self.send_sms_code();
                } else {
                    self.login_sms();
                }
            }
            LoginMethod::Sign => self.daily_sign(),
        }
    }

    /// `Enter` on the password form.
    fn login_password(&mut self) {
        let account = self.state.login.value(LoginField::Account);
        let password = self.state.login.value(LoginField::Password);
        if account.trim().is_empty() || password.is_empty() {
            self.state.login.error = Some("请输入账号与密码".to_string());
            self.state.login.notice = None;
            return;
        }

        let login = &mut self.state.login;
        login.loading = true;
        login.error = None;
        login.notice = None;

        let service = self.service.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let event = match service.login_password(&account, &password).await {
                Ok(info) => AuthEvent::Success(info),
                Err(error) => AuthEvent::Error(error.to_string()),
            };
            send_event(&sender, event.into());
        });
    }

    /// `Enter` on the phone box: ask the server to text a code to it. Nothing else sends one —
    /// opening the page, or a command that prefills the number, must not.
    fn send_sms_code(&mut self) {
        let phone = self.state.login.value(LoginField::Phone);
        if phone.trim().is_empty() {
            self.state.login.error = Some("请输入手机号".to_string());
            self.state.login.notice = None;
            return;
        }

        let login = &mut self.state.login;
        login.loading = true;
        login.error = None;
        login.notice = None;

        let service = self.service.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let event = match service.send_sms_code(&phone).await {
                Ok(()) => AuthEvent::SmsCodeSent(phone),
                Err(error) => AuthEvent::ActionResult(Err(format!("验证码发送失败: {error}"))),
            };
            send_event(&sender, event.into());
        });
    }

    /// `Enter` on the code box: log in with the number that is still in the form.
    fn login_sms(&mut self) {
        let phone = self.state.login.value(LoginField::Phone);
        let code = self.state.login.value(LoginField::Code);
        if phone.trim().is_empty() || code.trim().is_empty() {
            self.state.login.error = Some("请输入手机号与验证码".to_string());
            self.state.login.notice = None;
            return;
        }

        let login = &mut self.state.login;
        login.loading = true;
        login.error = None;
        login.notice = None;

        let service = self.service.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let event = match service.login_sms(&phone, &code).await {
                Ok(info) => AuthEvent::Success(info),
                Err(error) => AuthEvent::Error(error.to_string()),
            };
            send_event(&sender, event.into());
        });
    }

    /// `Enter` on the check-in tab. The service turns the response into a sentence the page can
    /// show as it is; nothing here re-reads or rewrites it.
    fn daily_sign(&mut self) {
        let login = &mut self.state.login;
        login.loading = true;
        login.error = None;
        login.notice = None;

        let service = self.service.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let result = match service.daily_sign().await {
                Ok(msg) => Ok(msg.msg),
                Err(error) => Err(format!("签到失败: {error}")),
            };
            send_event(&sender, AuthEvent::ActionResult(result).into());
        });
    }

    /// The code went out: keep the number the server accepted — the second step has to use the
    /// same one — and hand the caret to the box that is waiting for it.
    pub(super) fn handle_sms_code_sent(&mut self, phone: String) {
        let login = &mut self.state.login;
        login.loading = false;
        login.error = None;
        login.notice = Some(format!("验证码已发送到 {phone}"));
        login.fill(LoginField::Phone, &phone);
        login.focus = LoginField::Code;
    }

    /// A page action answered: the line goes in the page, in accent when it worked and in the
    /// error colour when it did not.
    pub(super) fn handle_action_result(&mut self, result: Result<String, String>) {
        let login = &mut self.state.login;
        login.loading = false;
        match result {
            Ok(message) => {
                login.notice = Some(message);
                login.error = None;
            }
            Err(error) => {
                login.error = Some(error);
                login.notice = None;
            }
        }
    }

    pub(super) fn handle_qr_created(&mut self, url: String, key: String) {
        self.state.login.loading = false;
        self.state.login.qr_url = url;
        self.state.login.qr_key = key.clone();
        self.state.login.qr_status_text = "等待扫码...".to_string();

        let service = self.service.clone();
        let sender = self.state.events.sender();
        tokio::spawn(async move {
            let mut scanned = false;
            for _ in 0..150 {
                sleep(Duration::from_secs(2)).await;
                match service.login_qr_check(&key).await {
                    Ok(resp) => match resp.code {
                        803 => {
                            match service.login_status().await {
                                Ok(info) => {
                                    send_event(&sender, AuthEvent::Success(info).into());
                                }
                                Err(e) => {
                                    send_event(&sender, AuthEvent::Error(e.to_string()).into());
                                }
                            }
                            return;
                        }
                        800 => {
                            send_event(
                                &sender,
                                AuthEvent::Error("二维码已过期，请重新生成".to_string()).into(),
                            );
                            return;
                        }
                        802 if !scanned => {
                            scanned = true;
                            send_event(
                                &sender,
                                AuthEvent::QRStatus("已扫码，请在手机上确认...".to_string()).into(),
                            );
                        }
                        802 => {}
                        _ => {}
                    },
                    Err(e) => {
                        send_event(&sender, AuthEvent::Error(e.to_string()).into());
                        return;
                    }
                }
            }
            send_event(&sender, AuthEvent::Error("登录超时".to_string()).into());
        });
    }

    pub(super) fn handle_qr_status(&mut self, text: String) {
        self.state.login.qr_status_text = text;
    }
}
