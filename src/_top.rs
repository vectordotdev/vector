// use crate::config;
// use std::collections::BTreeMap;
// use structopt::StructOpt;
// use url::Url;
// use vector_api_client::{
//     gql::{HealthQueryExt, TopologyQueryExt},
//     Client,
// };
//
// trait StatsWriter {
//     fn kb(&mut self, n: f64) -> String;
// }
//
// struct HumanWriter {
//     f: human_format::Formatter,
// }
//
// impl HumanWriter {
//     fn new() -> Self {
//         Self {
//             f: human_format::Formatter::new(),
//         }
//     }
// }
//
// impl StatsWriter for HumanWriter {
//     fn kb(&mut self, n: f64) -> String {
//         self.f.with_decimals(2).format(n)
//     }
// }
//
// struct LocaleWriter {
//     buf: num_format::Buffer,
// }
//
// impl LocaleWriter {
//     fn new() -> Self {
//         Self {
//             buf: num_format::Buffer::new(),
//         }
//     }
// }
//
// impl StatsWriter for LocaleWriter {
//     fn kb(&mut self, n: f64) -> String {
//         self.buf
//             .write_formatted(&(n as i64), &num_format::Locale::en);
//         self.buf.to_string()
//     }
// }
//
// fn new_formatter(humanize: bool) -> Box<dyn StatsWriter> {
//     if humanize {
//         Box::new(HumanWriter::new())
//     } else {
//         Box::new(LocaleWriter::new())
//     }
// }
//
// #[derive(StructOpt, Debug)]
// #[structopt(rename_all = "kebab-case")]
// pub struct Opts {
//     /// How often the screen refreshes (in milliseconds)
//     #[structopt(default_value = "500", short = "i", long)]
//     refresh_interval: i32,
//
//     #[structopt(short, long)]
//     url: Option<Url>,
//
//     #[structopt(short, long)]
//     human: bool,
// }
//
// /// Row type containing individual metrics stats
// pub struct TopologyRow {
//     topology_type: String,
//     events_processed: f64,
// }
//
// type TopologyTable = BTreeMap<String, TopologyRow>;
//
// /// (Re)draw the table with the latest stats
// // fn draw_table(t: &TopologyTable, mut formatter: Box<dyn StatsWriter>) {
// //     let mut table = Table::new();
// //     table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);
// //     table.set_titles(row!("NAME", "TYPE", r->"EVENTS"));
// //
// //     for (name, r) in t.iter() {
// //         table.add_row(row!(
// //             name,
// //             r.topology_type,
// //             r->formatter.kb(r.events_processed.clone())
// //         ));
// //     }
// //
// //     table.printstd();
// // }
//
// // async fn print_topology(client: &Client, formatter: Box<dyn StatsWriter>) -> Result<(), ()> {
// //     // Get initial topology, including aggregate stats, and build a table of rows
// //     let rows = client
// //         .topology_query()
// //         .await
// //         .map_err(|_| ())?
// //         .data
// //         .ok_or_else(|| ())?
// //         .topology
// //         .iter()
// //         .map(|d| {
// //             (
// //                 d.name.clone(),
// //                 TopologyRow {
// //                     topology_type: d.on.to_string(),
// //                     events_processed: d
// //                         .events_processed
// //                         .as_ref()
// //                         .map(|ep| ep.events_processed)
// //                         .unwrap_or(0.00),
// //                 },
// //             )
// //         })
// //         .collect::<TopologyTable>();
// //
// //     draw_table(&rows, formatter);
// //
// //     Ok(())
// // }
//
// pub struct StatefulTable<'a> {
//     state: TableState,
//     items: Vec<Vec<&'a str>>,
// }
//
// impl<'a> StatefulTable<'a> {
//     fn new() -> StatefulTable<'a> {
//         StatefulTable {
//             state: TableState::default(),
//             items: vec![
//                 vec!["Row11", "Row12", "Row13"],
//                 vec!["Row21", "Row22", "Row23"],
//                 vec!["Row31", "Row32", "Row33"],
//                 vec!["Row41", "Row42", "Row43"],
//                 vec!["Row51", "Row52", "Row53"],
//                 vec!["Row61", "Row62", "Row63"],
//                 vec!["Row71", "Row72", "Row73"],
//                 vec!["Row81", "Row82", "Row83"],
//                 vec!["Row91", "Row92", "Row93"],
//                 vec!["Row101", "Row102", "Row103"],
//                 vec!["Row111", "Row112", "Row113"],
//                 vec!["Row121", "Row122", "Row123"],
//                 vec!["Row131", "Row132", "Row133"],
//                 vec!["Row141", "Row142", "Row143"],
//                 vec!["Row151", "Row152", "Row153"],
//                 vec!["Row161", "Row162", "Row163"],
//                 vec!["Row171", "Row172", "Row173"],
//                 vec!["Row181", "Row182", "Row183"],
//                 vec!["Row191", "Row192", "Row193"],
//             ],
//         }
//     }
//     pub fn next(&mut self) {
//         let i = match self.state.selected() {
//             Some(i) => {
//                 if i >= self.items.len() - 1 {
//                     0
//                 } else {
//                     i + 1
//                 }
//             }
//             None => 0,
//         };
//         self.state.select(Some(i));
//     }
//
//     pub fn previous(&mut self) {
//         let i = match self.state.selected() {
//             Some(i) => {
//                 if i == 0 {
//                     self.items.len() - 1
//                 } else {
//                     i - 1
//                 }
//             }
//             None => 0,
//         };
//         self.state.select(Some(i));
//     }
// }
//
// fn draw() {}
//
// pub async fn cmd(opts: &Opts) -> exitcode::ExitCode {
//     let url = opts.url.clone().unwrap_or_else(|| {
//         let addr = config::api::default_bind().unwrap();
//         Url::parse(&*format!("http://{}/graphql", addr)).unwrap()
//     });
//
//     let client = Client::new(url);
//
//     // Check that the GraphQL server is reachable
//     // match client.health_query().await {
//     //     Ok(_) => (),
//     //     _ => {
//     //         eprintln!("Vector API server not reachable");
//     //         return exitcode::UNAVAILABLE;
//     //     }
//     // }
//
//     // Print initial topology
//     // TODO - make this auto-update!
//     // if print_topology(&client, new_formatter(opts.human))
//     //     .await
//     //     .is_err()
//     // {
//     //     eprintln!("Couldn't retrieve topology");
//     //     return exitcode::UNAVAILABLE;
//     // }
//
//     draw();
//
//     exitcode::OK
// }
