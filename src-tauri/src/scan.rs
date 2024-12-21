use std::path::Path;

use anyhow::Result;
use clap::Parser;
use tokio::runtime::Handle;
use zcash_warp::{
    coin::CoinDef,
    data::fb::{ConfigT, PaymentRequestT, RecipientT},
    db::{
        account::get_account_info,
        account_manager::{create_new_account, create_transparent_address},
        chain::{get_sync_height, truncate_scan},
        create_schema,
    },
    download_sapling_parameters,
    lwd::{broadcast, get_last_height},
    network::Network,
    pay::builder::init_sapling_prover_with_location,
    utils::{
        chain::reset_chain,
        pay::{prepare_payment, sign},
    },
    warp::sync::warp_synchronize,
};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long)]
    ntaddrs: u32,
    #[arg(long)]
    nzaddrs: u32,
    #[arg(long)]
    birth_height: u32,
    #[arg(long)]
    seed: String,
    #[arg(long)]
    destination: String,
    #[arg(long)]
    lwd_url: String,
}

fn set_url(zec: &mut CoinDef, lwd_url: &str) {
    let mut config = ConfigT::default();
    config.servers = Some(vec![lwd_url.to_string()]);
    zec.set_config(&config).unwrap();
    zec.set_path_password("../zwl-recover.db", "").unwrap();
}

#[tauri::command(rename_all = "snake_case")]
pub async fn init(
    seed: String,
    ntaddrs: u32,
    nzaddrs: u32,
    birth_height: u32,
    lwd_url: String,
) -> Result<(), String> {
    let do_init = tokio::task::block_in_place(|| {
        let handle = Handle::current();
        handle.block_on(async {
            download_sapling_parameters(None)?;
            let mut zec = CoinDef::from_network(0, Network::Main);
            set_url(&mut zec, &lwd_url);

            let mut connection = zec.connection()?;
            // 1. create db
            create_schema(&mut connection, "")?;

            // 1b. checkpoint at birth_height
            truncate_scan(&connection)?;
            let mut client = zec.connect_lwd()?;
            reset_chain(&zec.network, &mut connection, &mut client, birth_height).await?;

            // 2. create accounts
            for addr_index in 0..nzaddrs {
                create_new_account(
                    &zec.network,
                    &mut connection,
                    &format!("Z #{addr_index}"),
                    &seed,
                    addr_index,
                    birth_height,
                    false,
                    true,
                )?;
            }

            let tid = create_new_account(
                &zec.network,
                &mut connection,
                &format!("T"),
                &seed,
                0,
                birth_height,
                true,
                true,
            )?;

            // 3. create addresses
            let ai = get_account_info(&zec.network, &connection, tid)?;
            if let Some(ti) = &ai.transparent {
                for addr_index in 0..ntaddrs {
                    create_transparent_address(&zec.network, &connection, tid, 0, addr_index, ti)?;
                }
            }

            Ok::<_, anyhow::Error>(())
        })
    });

    do_init.map_err(|e| e.to_string())
}

#[tauri::command(rename_all = "snake_case")]
pub async fn run_scan(max_blocks: u32, lwd_url: String) -> Result<u32, String> {
    tokio::task::block_in_place(|| {
        let handle = Handle::current();
        handle.block_on(async {
            let r = run_scan_inner(max_blocks, lwd_url).await;
            r.map_err(|e| e.to_string())
        })
    })
}

async fn run_scan_inner(max_blocks: u32, lwd_url: String) -> Result<u32> {
    let mut zec = CoinDef::from_network(0, Network::Main);
    set_url(&mut zec, &lwd_url);
    let connection = zec.connection()?;
    let mut client = zec.connect_lwd()?;

    // 3b get latest height
    let end_height = get_last_height(&mut client).await?;
    println!("{end_height}");

    let db_height = get_sync_height(&connection)?.height;
    let end_height = end_height.min(db_height + max_blocks);

    // 4. synchronize
    warp_synchronize(&zec, end_height).await?;

    // 5. check balance
    let zbal = connection
        .query_row(
            "SELECT SUM(value) AS zbal FROM notes WHERE spent IS NULL AND orchard = 0",
            [],
            |r| r.get::<_, Option<u64>>(0),
        )?
        .unwrap_or_default();
    let tbal = connection
        .query_row(
            "SELECT SUM(value) AS tbal FROM utxos WHERE spent IS NULL",
            [],
            |r| r.get::<_, Option<u64>>(0),
        )?
        .unwrap_or_default();
    println!("{zbal} {tbal}");

    Ok(end_height)
}

#[tauri::command(rename_all = "snake_case")]
pub async fn do_sweep(destination: String, end_height: u32, lwd_url: String) -> Result<(), String> {
    let do_sweep = tokio::task::block_in_place(|| {
        let handle = Handle::current();
        handle.block_on(async {
            let home_dir = std::env::var("HOME").unwrap();
            init_sapling_prover_with_location(&Path::new(&home_dir).join(".zcash-params"))?;
            let mut zec = CoinDef::from_network(0, Network::Main);
            set_url(&mut zec, &lwd_url);
            let connection = zec.connection()?;
            let tid = connection.query_row("SELECT MAX(id_account) FROM accounts", [], |r| {
                r.get::<_, u32>(0)
            })?;

            // 6. make sweep txs
            let sweep = |table: &'static str, id_account: Option<u32>| {
                println!("{table}");
                let zec = zec.clone();
                let destination = destination.clone();
                async move {
                    let connection = zec.connection()?;
                    let mut client = zec.connect_lwd()?;
                    let zaccounts = {
                        match id_account {
                            Some(id_account) => vec![id_account],
                            None => {
                                let mut s = connection.prepare(&format!(
                        "WITH z_notes AS (SELECT * FROM notes WHERE orchard = 0 AND spent IS NULL)
                        SELECT account, SUM(value) FROM z_notes GROUP BY account;
                        "
                        ))?;
                                let rows = s.query_map([], |r| r.get::<_, u32>(0))?;
                                rows.collect::<Result<Vec<_>, _>>()?
                            }
                        }
                    };

                    for zaccount in zaccounts {
                        // 6a. create unsigned tx - show fees
                        println!("{zaccount}");
                        let bal = connection.query_row(
                    &format!("SELECT SUM(value) FROM {table} WHERE spent IS NULL AND account = ?1"),
                    [zaccount],
                    |r| r.get::<_, Option<u64>>(0),
                )?.unwrap_or_default();

                        if bal != 0 {
                            let mut recipient = RecipientT::default();
                            recipient.address = Some(destination.to_string());
                            recipient.amount = bal;
                            recipient.pools = 7;

                            let mut payment = PaymentRequestT::default();
                            payment.recipients = Some(vec![recipient]);
                            payment.src_pools = 7;
                            payment.sender_pay_fees = false;
                            payment.use_change = false;
                            payment.height = end_height;
                            payment.expiration = end_height + 40;
                            let summary = prepare_payment(
                                &zec.network,
                                &connection,
                                &mut client,
                                zaccount,
                                &payment,
                                "",
                            )
                            .await?;
                            println!("{}", summary.fee);

                            // 6b. sign tx
                            let txb = sign(&zec.network, &connection, &summary, end_height + 40)?;

                            let txid = broadcast(&mut client, end_height, &txb).await?;
                            println!("txid {txid}");
                        }
                    }
                    Ok::<_, anyhow::Error>(())
                }
            };

            sweep("utxos", Some(tid)).await?;
            sweep("notes", None).await?;

            Ok::<_, anyhow::Error>(())
        })
    });

    do_sweep.map_err(|e| e.to_string())
}
