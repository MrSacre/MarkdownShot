pub fn tag_of_name(name: &str) -> Option<&'static str> {
    let n = name.trim().trim_matches(|c: char| !c.is_alphanumeric() && c != '#' && c != '+').to_lowercase();
    Some(match n.as_str() {
        "gdscript" => "gdscript",
        "c#" | "csharp" | "c-sharp" => "csharp",
        "python" | "py" | "python3" => "python",
        "rust" | "rs" => "rust",
        "javascript" | "js" | "node" | "node.js" => "javascript",
        "typescript" | "ts" => "typescript",
        "jsx" => "jsx",
        "tsx" => "tsx",
        "bash" | "shell" | "sh" | "zsh" | "terminal" | "console" => "bash",
        "powershell" | "pwsh" => "powershell",
        "kotlin" => "kotlin",
        "java" => "java",
        "swift" => "swift",
        "objective-c" | "objc" => "objectivec",
        "c++" | "cpp" => "cpp",
        "c" => "c",
        "go" | "golang" => "go",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "html" => "html",
        "xml" => "xml",
        "css" => "css",
        "sql" => "sql",
        "php" => "php",
        "ruby" => "ruby",
        "glsl" | "shader" => "glsl",
        "vue" => "vue",
        "template" => "html",
        "scss" => "scss",
        "sass" => "sass",
        "less" => "less",
        "graphql" | "gql" => "graphql",
        "diff" | "patch" => "diff",
        "dockerfile" | "docker" => "dockerfile",
        "makefile" | "make" => "makefile",
        "ini" | "cfg" | "conf" => "ini",
        "nginx" => "nginx",
        "lua" => "lua",
        "r" => "r",
        "dart" => "dart",
        "elixir" | "ex" => "elixir",
        "haskell" | "hs" => "haskell",
        "scala" => "scala",
        "perl" | "pl" => "perl",
        "text" | "txt" | "plaintext" | "plain" => "text",
        "md" | "markdown" => "markdown",
        "jsonc" | "json5" => "json",
        "mjs" | "cjs" => "javascript",
        "mts" | "cts" => "typescript",
        _ => return None,
    })
}

pub fn tabs_line(text: &str) -> Option<&'static str> {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.is_empty() || words.len() > 5 {
        return None;
    }
    let common = ["text", "txt", "plain", "plaintext", "make", "template", "r", "c", "go", "conf", "cfg", "docker", "patch", "diff", "less", "pl", "ex", "hs", "md", "shell", "console", "terminal", "node"];
    if words.iter().all(|w| common.contains(&w.to_lowercase().as_str())) {
        return None;
    }
    let tags: Vec<&'static str> = words.iter().filter_map(|w| tag_of_name(w)).collect();
    (tags.len() == words.len()).then(|| tags[0])
}

pub fn guess(code: &str) -> Option<&'static str> {
    let lines: Vec<&str> = code.lines().map(|l| l.trim_end()).filter(|l| !l.trim().is_empty()).collect();
    if lines.is_empty() {
        return None;
    }
    let has = |s: &str| code.contains(s);
    let count = |s: &str| code.matches(s).count() as f32;
    let starts = |p: &str| lines.iter().filter(|l| l.trim_start().starts_with(p)).count() as f32;
    let n = lines.len() as f32;
    if has("error[E") || lines.iter().filter(|l| l.trim_start().starts_with("Compiling ") || l.trim_start().starts_with("Finished ")).count() >= 1 && !has("fn ") {
        return None;
    }
    let first = lines[0].trim_start();
    let mermaid = ["graph ", "flowchart ", "sequenceDiagram", "stateDiagram", "classDiagram", "erDiagram", "gantt", "pie", "journey", "gitGraph"];
    if mermaid.iter().any(|m| first.starts_with(m)) || has("{%") || has("$$") || count("$") >= 4.0 && !has("$ ") {
        return None;
    }
    if has("```") {
        return Some("markdown");
    }
    let mut s: Vec<(&'static str, f32)> = Vec::new();
    s.push((
        "rust",
        2.0 * starts("fn ") + 2.0 * count("pub fn ") + 1.5 * count("let mut ") + count("let ") * 0.5 + 2.0 * count("impl ")
            + count("::") * 0.5 + count("->") * 0.5 + 2.0 * count("println!") + 2.0 * count("#[derive") + 1.5 * count("&mut ")
            + count("use std") * 2.0 + count("Some(") * 0.5 + count("Ok(") * 0.5 + count("match ") * 0.5
            + count("struct ") * 0.7 + count("enum ") * 0.7 + count("&self") * 1.5 + count("unwrap()") * 1.0
            + count("let mut ") * 0.5 + 2.0 * count("String::from") + 2.0 * count("vec![") + count("i32") + count("u32") + count("usize")
            + count("&str") + count("Vec<") + count("Box<") + count("unsafe ") + count("#[") + count("print!(") * 2.0 + count("impl<") + count("dyn "),
    ));
    s.push((
        "python",
        2.0 * lines.iter().filter(|l| l.trim_start().starts_with("def ") && l.trim_end().ends_with(':')).count() as f32
            + 1.5 * lines.iter().filter(|l| l.starts_with("import ") && !l.ends_with(';') && !l.contains(" from '") && !l.contains(" from \"")).count() as f32
            + 1.5 * lines.iter().filter(|l| l.starts_with("from ") && l.contains(" import ")).count() as f32 + count("self.") * 0.7 + 2.0 * starts("elif ")
            + count("print(") * 0.7 + count("None") * 0.5 + count("True") * 0.3 + count("False") * 0.3 + count("__init__") * 2.0
            + 1.5 * starts("class ") * (count(":\n") > 0.0) as u8 as f32 + count("lambda ") + starts("@") * 0.5,
    ));
    s.push((
        "gdscript",
        2.5 * starts("func ") + 2.5 * starts("extends ") + 1.5 * starts("var ") + 2.0 * count("@export") + 2.0 * count("@onready")
            + count("Vector2") + count("Vector3") + count("_ready()") * 2.0 + count("_process(") * 1.5 + starts("signal ") * 2.0
            + starts("pass") * 0.5 + count("get_tree()") * 2.0 + count("get_node(") * 1.5 + count("get_child(") * 1.5 + count("add_child(") * 1.5
            + count("Engine.") * 2.0 + count("Packed") * 1.5 + count(".dot(") + count("_to(") * 0.5
            + if has(";") || has("{") { 0.0 } else { 0.5 * starts("var ") },
    ));
    s.push((
        "csharp",
        3.0 * count("using System") + 2.0 * count("public partial class") + 1.5 * count("public class ") + count("public override ")
            + count("namespace ") * 1.5 + count("GD.Print") * 2.0 + count("private ") * 0.7 + count("void ") * 0.7 + count("var ") * 0.3
            + count("; ") * 0.0 + count("[Export]") * 2.0,
    ));
    let jsx = count("</") + count("/>") + count("className=") + count("<View") * 2.0 + count("<Text") * 2.0;
    let js = 1.5 * count("const ") + count("let ") * 0.4 + count("=>") * 1.0 + 1.5 * count("function ") + count("console.") * 2.0
        + count("require(") * 2.0 + count("export default") * 2.0 + 2.0 * count("import ") * (has(" from '") || has(" from \"")) as u8 as f32
            + count("module.exports") * 2.0 + count("this.") * 0.5 + count("});") * 0.5
        + count("===") + count("useState") * 2.0 + count("async ") * 0.5 + count("await ") * 0.5;
    let ts = count(": string") + count(": number") + count(": boolean") + 1.5 * starts("interface ") + 1.5 * starts("type ")
        + count("Promise<") + 1.5 * starts("export interface") + count("readonly ") + count("): ") + count("?: ") + count("ReadonlyArray<");
    if jsx >= 3.0 && js >= 1.0 {
        s.push((if ts >= 2.0 { "tsx" } else { "jsx" }, js + jsx + ts));
    } else if ts >= 2.0 {
        s.push(("typescript", js + ts * 1.5));
    } else {
        s.push(("javascript", js));
    }
    let prompt = lines.iter().filter(|l| l.starts_with("$ ") || l.starts_with("% ")).count() as f32;
    let cmds = ["npm ", "npx ", "yarn ", "pnpm ", "pip ", "cargo ", "git ", "cd ", "sudo ", "brew ", "apt ", "apt-get ", "mkdir ", "curl ", "wget ", "docker ", "python ", "python3 ", "rustup ", "export ", "echo ", "chmod ", "adb ", "kill ", "mkdocs ", "magick ", "convert ", "node ", "rustc ", "dotnet ", "./", "ls ", "rm ", "cp ", "mv ", "cat "];
    let cmd_lines = lines
        .iter()
        .filter(|l| {
            let t = l.trim_start().trim_start_matches("$ ").trim_start_matches("% ");
            cmds.iter().any(|c| t.starts_with(c))
        })
        .count() as f32;
    let all_cmds = if cmd_lines + prompt >= n { 1.0 } else { 0.0 };
    s.push(("bash", 2.0 * prompt + 1.5 * cmd_lines + 3.0 * count("#!/bin/") + count("&&") * 0.5 + all_cmds));
    let t = code.trim_start();
    let json = if (t.starts_with('{') || t.starts_with('[')) && has("\":") { 2.0 + count("\":") * 0.5 } else { 0.0 };
    s.push(("json", json));
    let yaml_kv = lines
        .iter()
        .filter(|l| {
            let t = l.trim_start().trim_start_matches("- ");
            t.split_once(':').is_some_and(|(k, v)| {
                !k.is_empty() && k.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.') && (v.is_empty() || v.starts_with(' '))
            })
        })
        .count() as f32;
    let yaml_lines = lines
        .iter()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("- ") || t.starts_with('#')
        })
        .count() as f32
        + yaml_kv;
    let yaml_ok = !has(";") && !has("{") && !has("(") && yaml_kv >= 1.0 && yaml_lines >= n * 0.8;
    s.push(("yaml", if yaml_ok { 1.0 + yaml_kv } else { 0.0 }));
    let toml_sec = lines.iter().filter(|l| l.starts_with('[') && l.ends_with(']')).count() as f32;
    let toml_kv = lines.iter().filter(|l| l.contains(" = ") && !l.trim_end().ends_with(';')).count() as f32;
    s.push(("toml", if toml_sec >= 1.0 && toml_kv >= 1.0 && !has("fn ") { 2.0 * toml_sec + toml_kv } else { 0.0 }));
    if t.starts_with('<') {
        let tag = t[1..].chars().next().unwrap_or(' ');
        let score = 2.0 + count("</") + count("/>");
        if has("<?xml") || has("android:") || has("xmlns") {
            s.push(("xml", score + 1.0));
        } else if tag.is_ascii_uppercase() {
            s.push(("jsx", score + js));
        } else if !(jsx >= 3.0 && js >= 1.0) {
            s.push(("html", score));
        }
    }
    let css = lines.iter().filter(|l| {
        let t = l.trim();
        t.ends_with(';') && t.split_once(':').is_some_and(|(k, _)| !k.is_empty() && k.chars().all(|c| c.is_ascii_lowercase() || c == '-'))
    }).count() as f32;
    s.push(("css", if has("{") && css >= 2.0 && !has("(") { css * 1.2 } else { 0.0 }));
    s.push(("cpp", 3.0 * count("#include") + 2.0 * count("std::cout") + 2.0 * count("std::vector") + count("int main") * 2.0 + count("printf(") + count("nullptr") * 2.0));
    s.push(("kotlin", 2.0 * starts("fun ") + 1.5 * count("val ") + count("override fun") * 2.0 + count("companion object") * 2.0));
    s.push(("java", 2.0 * count("public static void main") + count("System.out") * 2.0 + count("@Override") * 1.5 + count("import java") * 2.0
        + 1.5 * lines.iter().filter(|l| (l.starts_with("import ") || l.starts_with("package ")) && l.ends_with(';')).count() as f32));
    s.push(("groovy", 2.0 * starts("implementation") + 2.0 * count("android {") + 2.0 * count("dependencies {") + 2.0 * count("apply plugin") + count("def ") * has(" = ") as u8 as f32 * 0.5));
    s.push(("swift", 2.0 * count("import UIKit") + 2.0 * count("import SwiftUI") + 1.5 * starts("func ") * has("->") as u8 as f32 * 0.0 + count("@objc") * 2.0 + count("guard let") * 2.0));
    s.push(("objectivec", 3.0 * count("@interface") + 3.0 * count("@implementation") + 2.0 * count("#import") + count("NSString") * 1.5));
    s.push(("sql", 2.0 * count("SELECT ") + 2.0 * count("CREATE TABLE") + count(" FROM ") + count("INSERT INTO") * 2.0));
    s.push(("go", 3.0 * lines.iter().filter(|l| l.starts_with("package ") && !l.ends_with(';')).count() as f32 + 2.0 * starts("func ") * has("{") as u8 as f32 * has(":=") as u8 as f32 + count(":=") * 0.7 + count("fmt.") * 2.0));
    s.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    let (best, sc) = s[0];
    let second = s.get(1).map(|v| v.1).unwrap_or(0.0);
    (sc >= 2.0 && sc >= second * 1.5 + 0.5).then_some(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognized_languages() {
        assert_eq!(guess("fn main() {\n    let mut v = Vec::new();\n    println!(\"{v:?}\");\n}"), Some("rust"));
        assert_eq!(guess("def f(x):\n    if x:\n        return None\n    elif y:\n        print(x)"), Some("python"));
        assert_eq!(guess("extends Node2D\n\nfunc _ready():\n    var speed = 10\n    pass"), Some("gdscript"));
        assert_eq!(guess("$ cargo new hello\n$ cd hello\n$ cargo run"), Some("bash"));
        assert_eq!(guess("const App = () => {\n  return (\n    <View style={s}>\n      <Text>Hi</Text>\n    </View>\n  );\n};"), Some("jsx"));
        assert_eq!(guess("{\n  \"name\": \"x\",\n  \"version\": \"1.0\"\n}"), Some("json"));
        assert_eq!(guess("plugins:\n  - search\n  - blog:\n      enabled: true"), Some("yaml"));
        assert_eq!(guess("[package]\nname = \"hello\"\nversion = \"0.1.0\""), Some("toml"));
        assert_eq!(guess("using System;\n\npublic partial class Player : Node\n{\n}"), Some("csharp"));
        assert_eq!(guess("error[E0382]: borrow of moved value: `s1`\n --> src/main.rs:5:28"), None);
        assert_eq!(guess("hello world"), None);
        assert_eq!(guess("graph LR\n  A[Start] --> B{Error?};"), None);
        assert_eq!(guess("{% if page.meta.comments %}\n  <h2>Comments</h2>\n{% endif %}"), None);
        assert_eq!(guess("type Result<T> = std::result::Result<T, std::io::Error>;"), Some("rust"));
        assert_eq!(guess("let s1 = String::from(\"hello\");"), Some("rust"));
        assert_eq!(guess("pip install mkdocs-material"), Some("bash"));
        assert_eq!(guess("<Animated.View style={{opacity: a}}>\n  <Text>Hi</Text>\n</Animated.View>"), Some("jsx"));
        assert_eq!(guess("<activity android:name=\".MainActivity\"\n    android:exported=\"true\" />"), Some("xml"));
        assert_eq!(guess("static isGrayscaleEnabled(): Promise<boolean>;"), Some("typescript"));
        assert_eq!(guess("package com.example;\n\nimport android.os.Bundle;"), Some("java"));
        assert_eq!(guess("var child = get_child(0)\nget_tree().root.add_child(scene)"), Some("gdscript"));
        assert_eq!(guess("Execution failed for task ':app:installDebug'.\n  com.android.builder.Exception: x"), None);
    }

    #[test]
    fn tabs() {
        assert_eq!(tabs_line("GDScript C#"), Some("gdscript"));
        assert_eq!(tabs_line("C# GDScript"), Some("csharp"));
        assert_eq!(tabs_line("See also the page"), None);
    }
}
