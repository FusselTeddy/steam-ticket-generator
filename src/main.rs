use std::{error::Error, io::{Read, Write}, time::Duration};

use base64::{prelude::BASE64_STANDARD, Engine as _};
use dialoguer::theme::ColorfulTheme;
use steamworks_sys::ESteamAPIInitResult;

fn main() {
    let app_id = dialoguer::Input::<u32>::with_theme(&ColorfulTheme::default())
        .with_prompt("Enter the App ID")
        .interact()
        .unwrap();

    if let Err(e) = generate_ticket(app_id) {
        eprintln!("Error while generating ticket: {}", e);
        eprintln!("Make sure you have the Steam client running and logged in. Check also that the account owns the game");
        return;
    }

    println!("Press Enter to exit...");
    std::io::stdin().read(&mut [0]).unwrap();
}

fn generate_ticket(app_id: u32) -> Result<(), Box<dyn Error>> {
    unsafe {
        std::env::set_var("SteamAppId", &app_id.to_string());
        std::env::set_var("SteamGameId", app_id.to_string());

        let init_result = steamworks_sys::SteamAPI_InitFlat(std::ptr::null_mut());
        steamworks_sys::SteamAPI_ManualDispatch_Init();

        match init_result {
            ESteamAPIInitResult::k_ESteamAPIInitResult_FailedGeneric => {
                return Err("Failed to initialize Steam API".into());
            },
            ESteamAPIInitResult::k_ESteamAPIInitResult_NoSteamClient => {
                return Err("Steam client is not running".into());
            }
            _ => {}
        }

        let user = steamworks_sys::SteamAPI_SteamUser_v023();

        steamworks_sys::SteamAPI_ISteamUser_RequestEncryptedAppTicket(user, std::ptr::null_mut(), 0);
        
        let pipe = steamworks_sys::SteamAPI_GetHSteamPipe();
        while run_callbacks(pipe).is_none() {
            std::thread::sleep(Duration::from_millis(100));
        }

        let ticket = {
            let mut ticket = vec![0; 2028];
            let mut ticket_len = 0;
            let success = steamworks_sys::SteamAPI_ISteamUser_GetEncryptedAppTicket(user, ticket.as_mut_ptr() as *mut _, 2048, &mut ticket_len);
            
            if !success {
                return Err("Failed to get encrypted app ticket, does the account own the game?".into());
            }

            ticket.truncate(ticket_len as usize);

            BASE64_STANDARD.encode(&ticket)
        };

        let steamid = steamworks_sys::SteamAPI_ISteamUser_GetSteamID(user);
        println!("Steam ID: {}", steamid);
        println!("Encrypted App Ticket: {}", ticket);

        let create_config_confirm = dialoguer::Confirm::with_theme(&ColorfulTheme::default())
            .with_prompt("Do you want to create configs.user.ini file?")
            .default(true)
            .interact()
            .unwrap();

        if create_config_confirm {
            match create_config(steamid, &ticket) {
                Ok(_) => println!("configs.user.ini created successfully."),
                Err(e) => eprintln!("Failed to create configs.user.ini: {}", e),
            }
        }
    }
    
    Ok(())
}

fn run_callbacks(pipe: i32) -> Option<u64> {
    unsafe {
        let mut call = None;

        steamworks_sys::SteamAPI_ManualDispatch_RunFrame(pipe);
        let mut callback = std::mem::zeroed();

        while steamworks_sys::SteamAPI_ManualDispatch_GetNextCallback(pipe, &mut callback) {
            if callback.m_iCallback == steamworks_sys::SteamAPICallCompleted_t_k_iCallback as i32 {
                let apicall = &mut *(callback.m_pubParam as *mut _ as *mut steamworks_sys::SteamAPICallCompleted_t);
                let mut apicall_result = vec![0; apicall.m_cubParam as usize];
                let mut failed = false;

                if steamworks_sys::SteamAPI_ManualDispatch_GetAPICallResult(
                    pipe,
                    apicall.m_hAsyncCall,
                    apicall_result.as_mut_ptr() as *mut _,
                    apicall.m_cubParam as _,
                    apicall.m_iCallback,
                    &mut failed
                ) {
                    if !failed && apicall.m_iCallback == steamworks_sys::EncryptedAppTicketResponse_t_k_iCallback as i32 {
                        call = Some(apicall.m_hAsyncCall)
                    }
                }
            }

            steamworks_sys::SteamAPI_ManualDispatch_FreeLastCallback(pipe);
        }

        call
    }
}

fn create_config(steamid: u64, ticket: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create("configs.user.ini")?;
    
    writeln!(file, "[user::general]")?;
    writeln!(file, "account_steamid={}", steamid)?;
    writeln!(file, "ticket={}", ticket)?;
    
    Ok(())
}
