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
    state: PullRequestState,
    #[serde(rename = "isDraft")]
    is_draft: bool,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum PullRequestState {
    Open,
    Closed,
    Merged,
}

impl PullRequestNode {
    fn colour(&self, text: &str) -> String {
        match (&self.state, self.is_draft) {
            (PullRequestState::Open, true) => text.fg::<xterm::DustyGray>().to_string(),
            (PullRequestState::Open, false) => text.fg::<xterm::PastelGreen>().to_string(),
            (PullRequestState::Merged, _) => text.fg::<xterm::StrikemasterPurple>().to_string(),
            (PullRequestState::Closed, _) => text.fg::<xterm::Maroon>().to_string(),
        }
    }
}

async fn fetch_recent_prs(
    owner: &str,
    repo: &str,
) -> Result<HashMap<String, Vec<PullRequestNode>>, Box<dyn std::error::Error>> {
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
                    state
                    isDraft
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

    let mut prs_by_branch: HashMap<String, Vec<PullRequestNode>> = HashMap::new();
    for pr in response.search.nodes {
        prs_by_branch
            .entry(pr.head_ref_name.clone())
            .or_default()
            .push(pr);
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
    branches.sort_by_key(|b| std::cmp::Reverse(b.1));
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
            Some(prs) => println!(
                "{branch:width$} {}",
                prs.iter()
                    .map(|pr| pr.colour(&format!(" {}", pr.title)))
                    .collect::<Vec<String>>()
                    .join(", ")
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
