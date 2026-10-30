// A release's notes: the CHANGELOG section, which is Markdown. Only the
// part the CHANGELOG uses is read: headings (`###`), bullets (`-`, with
// indented continuation lines), paragraphs, and inside them `code`,
// **bold** and [links](…), whose text is kept and whose address is not
// (the CHANGELOG links point into the repository). Anything else is text.
// Elements are built from the parts, never from HTML, so the notes cannot
// put markup on the screen.
import { Fragment, useMemo } from "react";

export type NoteSpan = { kind: "text" | "code" | "strong"; text: string };

export type NoteBlock =
  | { kind: "heading"; spans: NoteSpan[] }
  | { kind: "paragraph"; spans: NoteSpan[] }
  | { kind: "list"; items: NoteSpan[][] };

const INLINE = /`([^`]+)`|\*\*(.+?)\*\*|\[([^\]]+)\]\(([^)\s]*)\)/g;

/** One line's text, code and bold runs. */
export function parseSpans(text: string): NoteSpan[] {
  const spans: NoteSpan[] = [];
  const push = (kind: NoteSpan["kind"], value: string) => {
    if (value === "") return;
    const last = spans[spans.length - 1];
    if (kind === "text" && last?.kind === "text") last.text += value;
    else spans.push({ kind, text: value });
  };
  let at = 0;
  for (const match of text.matchAll(INLINE)) {
    push("text", text.slice(at, match.index));
    if (match[1] !== undefined) push("code", match[1]);
    else if (match[2] !== undefined) push("strong", match[2]);
    else push("text", match[3] ?? "");
    at = match.index + match[0].length;
  }
  push("text", text.slice(at));
  return spans;
}

const HEADING = /^ {0,3}#{1,6}\s+(.*?)(?:\s+#+)?\s*$/;
const BULLET = /^\s*[-*+]\s+(.*)$/;

/** The notes as blocks, in order. Blank input gives no blocks. */
export function parseNotes(markdown: string): NoteBlock[] {
  const blocks: NoteBlock[] = [];
  let paragraph: string[] = [];
  let list: string[] | null = null;
  // A blank line ends a paragraph; a list goes on past it while bullets follow.
  let blank = false;
  const endParagraph = () => {
    if (paragraph.length > 0) blocks.push({ kind: "paragraph", spans: parseSpans(paragraph.join(" ")) });
    paragraph = [];
  };
  const endList = () => {
    if (list) blocks.push({ kind: "list", items: list.map(parseSpans) });
    list = null;
  };
  for (const raw of markdown.replace(/\r\n?/g, "\n").split("\n")) {
    const line = raw.trimEnd();
    if (line.trim() === "") {
      endParagraph();
      blank = true;
      continue;
    }
    const heading = HEADING.exec(line);
    const bullet = heading ? null : BULLET.exec(line);
    if (heading) {
      endParagraph();
      endList();
      blocks.push({ kind: "heading", spans: parseSpans(heading[1] ?? "") });
    } else if (bullet) {
      endParagraph();
      (list ??= []).push(bullet[1] ?? "");
    } else if (list && !blank && /^\s/.test(line)) {
      // An indented line under a bullet continues it.
      list[list.length - 1] += ` ${line.trim()}`;
    } else {
      endList();
      paragraph.push(line.trim());
    }
    blank = false;
  }
  endParagraph();
  endList();
  return blocks;
}

function Spans({ spans }: { spans: NoteSpan[] }) {
  return (
    <>
      {spans.map((span, index) =>
        span.kind === "code" ? <code key={index}>{span.text}</code> : span.kind === "strong" ? <strong key={index}>{span.text}</strong> : <Fragment key={index}>{span.text}</Fragment>,
      )}
    </>
  );
}

export function ReleaseNotes({ markdown, id }: { markdown: string; id?: string }) {
  const blocks = useMemo(() => parseNotes(markdown), [markdown]);
  return (
    <div className="update-notes" data-ui={id}>
      {blocks.map((block, index) => {
        if (block.kind === "heading")
          return (
            <h3 key={index} className="update-notes__heading">
              <Spans spans={block.spans} />
            </h3>
          );
        if (block.kind === "list")
          return (
            <ul key={index} className="update-notes__list">
              {block.items.map((item, at) => (
                <li key={at}>
                  <Spans spans={item} />
                </li>
              ))}
            </ul>
          );
        return (
          <p key={index}>
            <Spans spans={block.spans} />
          </p>
        );
      })}
    </div>
  );
}
