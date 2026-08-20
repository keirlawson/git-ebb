use owo_colors::OwoColorize;
use owo_colors::colors::*;

use std::collections::HashMap;
use std::env;
use std::process::ExitCode;

fn repo_slug(repo: &gix::Repository) -> Result<(String, String), Box<dyn std::error::Error>> {
    let remote = repo
        .find_default_remote(gix::remote::Direction::Fetch)
        .ok_or("no fetch remote configured")??;
    let url = remote
        .url(gix::remote::Direction::Fetch)
        .ok_or("remote has no fetch URL")?;

    let path = url.path.to_string();
    let path = path.trim_start_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);

    let (owner, name) = path
        .rsplit_once('/')
        .ok_or_else(|| format!("could not parse owner/repo from remote path '{path}'"))?;
    Ok((owner.to_string(), name.to_string()))
}

#[derive(serde::Deserialize)]
struct SearchData {
    search: Search,
}

#[derive(serde::Deserialize)]
struct Search {
    nodes: Vec<PullRequestNode>,
}

#[derive(serde::Deserialize)]
struct PullRequestNode {
    title: String,
    #[serde(rename = "headRefName")]
    head_ref_name: String,
}

async fn fetch_recent_prs(
    owner: &str,
    repo: &str,
) -> Result<HashMap<String, Vec<String>>, Box<dyn std::error::Error>> {
    let token = env::var("EBB_TOKEN")
        .or_else(|_| env::var("GITHUB_TOKEN"))
        .map_err(|_| "Neither EBB_TOKEN nor GITHUB_TOKEN environment variables set")?;

    let client = octocrab::Octocrab::builder()
        .personal_token(token)
        .build()?;

    let query = "query($q: String!) {
        search(query: $q, type: ISSUE, first: 100) {
            nodes {
                ... on PullRequest {
                    title
                    headRefName
                }
            }
        }
    }";
    let body = serde_json::json!({
        "query": query,
        "variables": {
            "q": format!("repo:{owner}/{repo} is:pr author:@me sort:updated-desc"),
        },
    });

    let response: SearchData = client.graphql(&body).await?;

    let mut prs_by_branch: HashMap<String, Vec<String>> = HashMap::new();
    for pr in response.search.nodes {
        prs_by_branch
            .entry(pr.head_ref_name)
            .and_modify(|v| {
                v.push(pr.title.clone());
            })
            .or_insert_with(|| vec![pr.title]);
    }

    Ok(prs_by_branch)
}

fn local_branches(repo: &gix::Repository) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let mut branches = Vec::new();
    for reference in repo.references()?.local_branches()? {
        let mut reference = reference.map_err(|e| e.to_string())?;
        let name = reference.name().shorten().to_string();
        let time = reference.peel_to_commit()?.committer()?.time()?.seconds;
        branches.push((name, time));
    }
    branches.sort_by(|a, b| b.1.cmp(&a.1));
    Ok(branches.into_iter().map(|(name, _)| name).collect())
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let repo = gix::discover(".")?;
    let (owner, name) = repo_slug(&repo)?;
    let prs_by_branch = fetch_recent_prs(&owner, &name).await?;
    let branches = local_branches(&repo)?;

    let width = branches.iter().map(String::len).max().unwrap_or(0);

    for branch in &branches {
        match prs_by_branch.get(branch) {
            Some(titles) => println!(
                "{branch:width$} {}",
                titles
                    .iter()
                    .map(|title| format!(" {}", title))
                    .collect::<Vec<String>>()
                    .join(", ")
                    .fg::<xterm::StrikemasterPurple>()
            ),
            None => println!("{branch:width$}"),
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}
