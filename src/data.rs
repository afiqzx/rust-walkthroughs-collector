use std::{collections::HashMap, fs::File, io::Write, path::Path};

use log::info;
use rayon::prelude::*;
use tl::HTMLTag;
use tl::NodeHandle;

use crate::{Result, WalkthroughArticle, WalkthroughArticlesByIssueLink};

pub fn get_local_walkthrough_articles<P>(path: P) -> Result<Option<WalkthroughArticlesByIssueLink>>
where
    P: AsRef<Path>,
{
    if !path.as_ref().exists() {
        return Ok(None);
    }

    let file_content = String::from_utf8(std::fs::read(path)?)?;
    if file_content.is_empty() {
        return Ok(None);
    }

    Ok(serde_json::from_str(&file_content)?)
}

pub fn scrape_walkthrough_articles_by_issue_link() -> Result<WalkthroughArticlesByIssueLink> {
    // get Past Issues page https://this-week-in-rust.org/blog/archives/index.html
    // parse into Dom
    // get all past issue links
    let past_issues_page_html =
        get_page_html("https://this-week-in-rust.org/blog/archives/index.html");
    let past_issues_page_dom =
        tl::parse(&past_issues_page_html, tl::ParserOptions::default()).unwrap();
    let issue_links = get_all_issue_links(&past_issues_page_dom);

    // iterate through all issue links & get walkthrough articles
    let walkthrough_articles = issue_links.par_iter().map(|issue_link| {
        info!("getting past issue - {issue_link}");
        let issue_page_html = get_page_html(&issue_link);
        let issue_page_dom = tl::parse(&issue_page_html, tl::ParserOptions::default()).unwrap();

        (
            issue_link,
            get_walkthrough_articles(&issue_page_dom)
                .expect(format!("failed to get walkthrough_article for {issue_link}").as_str()),
        )
    });

    Ok(walkthrough_articles
        .fold(
            || HashMap::new(),
            |mut map, (issue_link, walkthrough_articles)| {
                map.insert(issue_link.clone(), walkthrough_articles);
                map
            },
        )
        .reduce(
            || HashMap::new(),
            |mut map, m| {
                for (issue_link, walkthrough_articles) in m {
                    map.insert(issue_link, walkthrough_articles);
                }
                map
            },
        ))
}

fn get_page_html(url: &str) -> String {
    let res = reqwest::blocking::get(url).unwrap();
    return res.text().unwrap();
}

fn get_all_issue_links(past_issues_page_dom: &tl::VDom) -> Vec<String> {
    let dom_parser = past_issues_page_dom.parser();

    // find all `div` with class `.post-title`, which includes the link for each issues
    let mut issue_links = Vec::new();
    for div_handle in past_issues_page_dom
        .query_selector("div.post-title")
        .unwrap()
    {
        // parse div into dom
        let div_node = div_handle.get(dom_parser).unwrap();
        let div_html = div_node.inner_html(dom_parser);
        let div_dom = tl::parse(div_html.as_ref(), tl::ParserOptions::default()).unwrap();

        // find `a` in the div and get its `href` attribute's value (the link)
        // colelct into `issue_links` Vec
        let a_handle = div_dom.query_selector("a").unwrap().next().unwrap();
        let a_node = a_handle.get(div_dom.parser()).unwrap();
        match a_node {
            tl::Node::Tag(a_tag_node) => {
                let attrs = a_tag_node.attributes();
                let href = attrs.get("href").unwrap().unwrap();
                issue_links.push(href.as_utf8_str().to_string());
            }
            _ => {}
        }
    }

    issue_links
}

fn get_walkthrough_articles(issue_page_dom: &tl::VDom) -> Result<Vec<WalkthroughArticle>> {
    let parser = issue_page_dom.parser();

    let rust_walkthroughs_title_handle = issue_page_dom
        .query_selector("#rust-walkthroughs")
        .ok_or("failed to query for #rust-walkthroughs")?
        .next();

    if rust_walkthroughs_title_handle.is_none() {
        return Ok(Vec::new());
    };

    let rust_walkthroughs_title_handle = rust_walkthroughs_title_handle.unwrap();
    let walkthrough_list_handle = NodeHandle::new(rust_walkthroughs_title_handle.get_inner() + 4);
    let walkthrough_list_node = walkthrough_list_handle
        .get(parser)
        .ok_or("failed to get walkthrough_list_node")?;

    let walkthrough_list_html = walkthrough_list_node.inner_html(parser);
    let walkthrough_list_dom =
        tl::parse(walkthrough_list_html.as_ref(), tl::ParserOptions::default())?;

    let list_item_handles = walkthrough_list_dom
        .query_selector("li")
        .ok_or("no <li> elements inside <ul>")?;

    let mut ret = vec![];
    for list_item_handle in list_item_handles {
        let list_item_node = list_item_handle
            .get(walkthrough_list_dom.parser())
            .ok_or("failed to get list_item_node")?;

        let list_title = list_item_node
            .inner_text(walkthrough_list_dom.parser())
            .to_string();

        let list_item_html = list_item_node.inner_html(walkthrough_list_dom.parser());
        let list_item_dom = tl::parse(list_item_html.as_ref(), tl::ParserOptions::default())?;

        let maybe_list_href = list_item_dom
            .query_selector("a")
            .map(|mut iter| iter.next())
            .flatten()
            .map(|handle| handle.get(list_item_dom.parser()))
            .flatten()
            .map(tl::Node::as_tag)
            .flatten()
            .map(HTMLTag::attributes)
            .map(|tag| tag.get("href"))
            .flatten()
            .flatten()
            .map(|href| href.as_utf8_str().to_string());

        if maybe_list_href.is_none() {
            continue;
        }

        let list_href = maybe_list_href.unwrap();
        ret.push(WalkthroughArticle {
            title: list_title,
            link: list_href,
        });
    }

    Ok(ret)
}

pub fn store_locally<P>(local_path: P, articles: &WalkthroughArticlesByIssueLink) -> Result<()>
where
    P: AsRef<Path>,
{
    let mut file = File::create(local_path)?;
    file.write_all(&serde_json::to_vec(articles)?)?;

    info!("stored result to $HOME/.rust_walkthrough_articles");
    Ok(())
}