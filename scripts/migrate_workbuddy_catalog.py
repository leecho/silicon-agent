#!/usr/bin/env python3
"""把 WorkBuddy expert plugins 迁移成 silicon-agent 广场内置目录（builtin-catalog）。

单 expert(expertType=agent) → builtin-catalog/agents/<plugin>.md
team(expertType=team)       → builtin-catalog/teams/<plugin>/catalog.json + agents/*.md

显示信息取 plugin.json 的 zh 字段；system prompt 取各 agent .md 去 frontmatter 后的正文；
头像取 agent .md frontmatter 的 emoji（无则省略，前端回退图标）。
"""
import json
import os
import re
import shutil
import sys

SRC = "/Users/liqiu/.workbuddy/plugins/marketplaces/experts/plugins"
DST = os.path.join(os.path.dirname(__file__), "..", "src-tauri", "builtin-catalog")
DST = os.path.abspath(DST)

CATEGORY = {
    "01-ProductDesign": "产品设计",
    "02-Engineering": "技术工程",
    "03-GameSpatial": "游戏空间",
    "04-DataAI": "数据智能",
    "05-Marketing": "营销增长",
    "06-ContentCreative": "内容创作",
    "07-SalesCommerce": "销售商务",
    "08-FinanceInvestment": "金融投资",
    "12-IndustryConsultant": "行业顾问",
}
# categoryId 缺失时按 plugin 名兜底。
CATEGORY_FALLBACK = {
    "equity-research": "金融投资",
    "software-company": "技术工程",
}


def zh(v, default=""):
    """从 {zh,en} 或字符串取中文。"""
    if isinstance(v, dict):
        return v.get("zh") or v.get("en") or default
    if isinstance(v, str):
        return v
    return default


def strip_fm(text):
    """去掉 markdown frontmatter，返回正文。"""
    if text.lstrip().startswith("---"):
        # 去掉第一段 --- ... --- 。
        m = re.match(r"^\s*---\s*\n.*?\n---\s*\n?(.*)$", text, re.DOTALL)
        if m:
            return m.group(1).strip() + "\n"
    return text.strip() + "\n"


def fm_emoji(text):
    """从 agent .md frontmatter 提取 emoji 字段。"""
    m = re.search(r"^\s*emoji:\s*(.+?)\s*$", text, re.MULTILINE)
    if m:
        val = m.group(1).strip().strip('"').strip("'")
        # 仅当是 emoji（非路径/英文）时采用。
        if val and "/" not in val and not re.search(r"[A-Za-z]", val):
            return val
    return None


def fm_field(text, key):
    """从 frontmatter 取某字段（单行）。"""
    m = re.search(r"^\s*" + re.escape(key) + r":\s*(.+?)\s*$", text, re.MULTILINE)
    return m.group(1).strip().strip('"').strip("'") if m else None


def heading_name(body):
    """正文首个 markdown 标题里的身份名（截到 （/·/- 等分隔符前）。"""
    m = re.search(r"^#{1,3}\s+(.+)$", body, re.MULTILINE)
    if not m:
        return None
    t = re.split(r"[（(·\-—|/]", m.group(1).strip())[0].strip()
    return t or None


def heading_paren(body):
    """正文首个标题里第一个中文括号内容（常是职业）。"""
    m = re.search(r"^#{1,3}\s+.+$", body, re.MULTILINE)
    if not m:
        return None
    pm = re.search(r"[（(]([^）)]+)[）)]", m.group(0))
    if pm and re.search(r"[一-鿿]", pm.group(1)):
        return pm.group(1).strip()
    return None


def yaml_escape(s):
    """单行值：折叠换行、去首尾空白。"""
    return " ".join(s.replace("\r", " ").split())


def read(path):
    with open(path, "r", encoding="utf-8") as f:
        return f.read()


def write(path, content):
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w", encoding="utf-8") as f:
        f.write(content)


def agent_md_path(plugin_dir, ref_or_id):
    """解析 agent 文件路径：支持 './agents/x.md' 或裸 id。"""
    if ref_or_id.endswith(".md"):
        return os.path.join(plugin_dir, ref_or_id.lstrip("./"))
    return os.path.join(plugin_dir, "agents", ref_or_id + ".md")


def copy_tree_no_symlink(src, dst):
    """递归复制目录，跳过 symlink（包内常含指向源机绝对路径的断链，如 /Users/x/.qclaw/...）。"""
    os.makedirs(dst, exist_ok=True)
    for entry in sorted(os.listdir(src)):
        s = os.path.join(src, entry)
        d = os.path.join(dst, entry)
        if os.path.islink(s):
            continue
        if os.path.isdir(s):
            copy_tree_no_symlink(s, d)
        else:
            shutil.copy2(s, d)


def copy_skills(pdir, pj, out_dir):
    """把包声明的 skills 整目录复制进 out_dir/skills/<skill>/（跳过 symlink）。返回复制成功数。"""
    n = 0
    for rel in pj.get("skills") or []:
        rel = rel.strip().lstrip("./")
        src = os.path.join(pdir, rel)
        if not os.path.isdir(src):
            print(f"    跳过 skill（缺目录）{rel}")
            continue
        name = os.path.basename(rel.rstrip("/"))
        copy_tree_no_symlink(src, os.path.join(out_dir, "skills", name))
        n += 1
    return n


def build_agent_fm(name, description, category, scenario, tags, order, profession, avatar, quick_prompts, catalog_id, role=None, display_name=None):
    lines = ["---", f"name: {yaml_escape(name)}"]
    if description:
        lines.append(f"description: {yaml_escape(description)}")
    if display_name and display_name != name:
        lines.append(f"display_name: {yaml_escape(display_name)}")
    if role:
        lines.append(f"role: {role}")
    lines.append("model: main")
    if category:
        lines.append(f"category: {category}")
    if scenario:
        lines.append(f"scenario: {scenario}")
    if tags:
        lines.append("tags: [" + ", ".join(yaml_escape(t) for t in tags) + "]")
    lines.append("featured: true")
    lines.append(f"order: {order}")
    if profession:
        lines.append(f"profession: {yaml_escape(profession)}")
    if avatar:
        lines.append(f"avatar: {avatar}")
    if quick_prompts:
        lines.append("quick_prompts: " + json.dumps(quick_prompts, ensure_ascii=False))
    if catalog_id:
        lines.append(f"catalog_id: {catalog_id}")
    lines.append("---")
    return "\n".join(lines) + "\n\n"


def main():
    # 重生成前清空旧目录（现有条目全来自 workbuddy，无手写内容）。
    for sub in ("agents", "teams"):
        p = os.path.join(DST, sub)
        if os.path.isdir(p):
            shutil.rmtree(p)
    plugins = sorted(d for d in os.listdir(SRC) if os.path.isdir(os.path.join(SRC, d)))
    n_agents = n_teams = 0
    order = 0
    for p in plugins:
        pdir = os.path.join(SRC, p)
        pj_path = os.path.join(pdir, ".codebuddy-plugin", "plugin.json")
        if not os.path.isfile(pj_path):
            print(f"  skip {p}: no plugin.json")
            continue
        pj = json.loads(read(pj_path))
        order += 10
        etype = pj.get("expertType", "agent")
        category = CATEGORY.get(pj.get("categoryId", ""), CATEGORY_FALLBACK.get(p, "其他"))
        scenario = category
        display = zh(pj.get("displayName"), p)
        desc = zh(pj.get("displayDescription")) or zh(pj.get("description"))
        profession = zh(pj.get("profession"))
        tags = [zh(t) for t in (pj.get("tags") or []) if zh(t)]
        qps = [zh(q) for q in (pj.get("quickPrompts") or []) if zh(q)]

        if etype == "team":
            members = pj.get("members") or []
            team_info = pj.get("teamInfo") or {}
            lead_id = team_info.get("leadAgent")
            # 无 members[] 则从 teamInfo 兜底构造。
            if not members:
                members = [{"id": lead_id, "role": "lead"}] + [
                    {"id": m, "role": "member"} for m in (team_info.get("memberAgents") or [])
                ]
            out_dir = os.path.join(DST, "teams", p)
            wrote = 0
            lead_body = ""
            for m in members:
                mid = m.get("id")
                if not mid:
                    continue
                src = agent_md_path(pdir, mid)
                if not os.path.isfile(src):
                    print(f"    [{p}] 缺成员文件 {mid}")
                    continue
                raw = read(src)
                body = strip_fm(raw)
                emoji = fm_emoji(raw)
                role = m.get("role") or ("lead" if mid == lead_id else "member")
                if role == "lead":
                    lead_body = body
                mprof = zh(m.get("profession")) or heading_paren(body)
                mname = zh(m.get("name")) or heading_name(body) or mid
                fm = build_agent_fm(
                    name=mid,
                    description=mprof or fm_field(raw, "description") or "",
                    category=None, scenario=None, tags=[], order=0,
                    profession=mprof,
                    avatar=emoji,
                    quick_prompts=[],
                    catalog_id=None,
                    role=role,
                    display_name=mname,
                )
                write(os.path.join(out_dir, "agents", mid + ".md"), fm + body)
                wrote += 1
            # 团队 displayName 缺失时回退 lead 正文标题。
            if display == p:
                display = heading_name(lead_body) or p
            meta = {
                "name": p,
                "displayName": display,
                "description": desc,
                "category": category,
                "scenario": scenario,
                "tags": tags,
                "featured": True,
                "order": order,
                "quickPrompts": qps,
            }
            write(os.path.join(out_dir, "catalog.json"), json.dumps(meta, ensure_ascii=False, indent=2) + "\n")
            n_skills = copy_skills(pdir, pj, out_dir)
            print(f"  team {p}: {wrote} agents, {n_skills} skills")
            n_teams += 1
        else:
            # 单 expert。
            refs = pj.get("agents") or []
            agent_name = pj.get("agentName")
            src = None
            if refs:
                src = agent_md_path(pdir, refs[0])
            elif agent_name:
                src = agent_md_path(pdir, agent_name)
            if not src or not os.path.isfile(src):
                print(f"  skip {p}: no agent file")
                continue
            raw = read(src)
            body = strip_fm(raw)
            emoji = fm_emoji(raw)
            # 清单缺 displayName/description/profession 时回退正文标题与 frontmatter。
            if display == p:
                display = heading_name(body) or fm_field(raw, "name") or p
            if not desc:
                desc = fm_field(raw, "description") or ""
            if not profession:
                profession = heading_paren(body)
            # name 用唯一 slug、display_name 放中文，避免两个同显示名（如「福帮手」）落库时撞名。
            fm = build_agent_fm(
                name=p,
                description=desc,
                category=category,
                scenario=scenario,
                tags=tags,
                order=order,
                profession=profession,
                avatar=emoji,
                quick_prompts=qps,
                catalog_id=p,
                display_name=display,
            )
            # 目录格式：agents/<id>/agent.md + skills/（携带技能需要目录承载）。
            out_dir = os.path.join(DST, "agents", p)
            write(os.path.join(out_dir, "agent.md"), fm + body)
            n_skills = copy_skills(pdir, pj, out_dir)
            print(f"  agent {p} → {display}（{n_skills} skills）")
            n_agents += 1

    print(f"\n完成：{n_agents} 智能体 + {n_teams} 团队 → {DST}")


if __name__ == "__main__":
    main()
