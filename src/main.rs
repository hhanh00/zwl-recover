use std::path::Path;

use anyhow::Result;
use clap::Parser;
use zcash_warp::{
    coin::CoinDef,
    data::fb::{ConfigT, PaymentRequestT, RecipientT},
    db::{
        account::get_account_info,
        account_manager::{create_new_account, create_transparent_address},
        create_schema,
    },
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

#[tokio::main]
async fn main() -> Result<()> {
    // let seed = "total below tumble rack treat monkey climb service erase rotate ranch fitness warrior sweet scorpion into note minimum wrist because only lottery mule swim";
    // let birth_height = 2757209;
    // let ntaddrs = 5;
    // let nzaddrs = 3;
    // let destination = "u1c2v7ugwccldxzawuzgmt05nt258kkh76hc9rzpahyy3admqd4el30en4pka5zmjkxrte37qszwch4w9kyux393unsu6ftrpz0cvxxlm4";
    // let lwd_url = "https://zec.rocks";

    let args = Args::parse();
    let Args {
        seed,
        birth_height,
        ntaddrs,
        nzaddrs,
        destination,
        lwd_url,
    } = args;

    let home_dir = std::env::var("HOME").unwrap();
    init_sapling_prover_with_location(&Path::new(&home_dir).join(".zcash-params"))?;

    let mut zec = CoinDef::from_network(0, Network::Main);
    zec.set_path_password("zwl-recover.db", "")?;
    let mut config = ConfigT::default();
    config.servers = Some(vec![lwd_url.to_string()]);
    zec.set_config(&config)?;

    let mut connection = zec.connection()?;
    // 1. create db
    create_schema(&mut connection, "")?;

    // 1b. checkpoint at birth_height
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

    // 3b get latest height
    let end_height = get_last_height(&mut client).await?;
    println!("{end_height}");

    // 4. synchronize
    warp_synchronize(&zec, end_height).await?;

    // 5. check balance
    let zbal = connection
        .query_row(
            "SELECT SUM(value) FROM notes WHERE spent IS NULL",
            [],
            |r| r.get::<_, Option<u64>>(0),
        )?
        .unwrap_or_default();
    let tbal = connection
        .query_row(
            "SELECT SUM(value) FROM utxos WHERE spent IS NULL",
            [],
            |r| r.get::<_, Option<u64>>(0),
        )?
        .unwrap_or_default();
    println!("{zbal} {tbal}");

    // 6. make sweep txs
    let sweep = |table: &'static str, id_account: Option<u32>| {
        let zec = zec.clone();
        let destination = destination.clone();
        async move {
            let mut client = zec.connect_lwd()?;
            let connection = zec.connection()?;
            let zaccounts = match id_account {
                Some(id_account) => vec![id_account],
                None => {
                    let mut s = connection.prepare(&format!(
                        "SELECT account, SUM(value) AS bal FROM {table} WHERE spent IS NULL
                GROUP BY account"
                    ))?;
                    let rows = s.query_map([], |r| r.get::<_, u32>(0))?;
                    rows.collect::<Result<Vec<_>, _>>()?
                }
            };

            for zaccount in zaccounts {
                // 6a. create unsigned tx - show fees
                println!("{zaccount}");
                let bal = connection.query_row(
                    &format!("SELECT SUM(value) FROM {table} WHERE spent IS NULL AND account = ?1"),
                    [zaccount],
                    |r| r.get::<_, u64>(0),
                )?;

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
            Ok::<_, anyhow::Error>(())
        }
    };

    sweep("utxos", Some(tid)).await?;
    sweep("notes", None).await?;

    Ok(())
}
