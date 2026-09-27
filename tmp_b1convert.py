import re

def scan_block(s, start):
    """start = index of '{' after 'Diag '. Return end index (inclusive) of matching '}'."""
    depth = 0
    i = start
    n = len(s)
    while i < n:
        c = s[i]
        if c == '"':
            i += 1
            while i < n:
                if s[i] == '\\':
                    i += 2
                    continue
                if s[i] == '"':
                    break
                i += 1
        elif c == '{':
            depth += 1
        elif c == '}':
            depth -= 1
            if depth == 0:
                return i
        i += 1
    raise ValueError("unbalanced")

def convert(path, stage, self_file_expr, self_helper):
    s = open(path, encoding='utf-8').read()
    out = []
    pos = 0
    count_self = count_at = 0
    while True:
        m = re.compile(r'Diag\s*\{').search(s, pos)
        if not m:
            out.append(s[pos:])
            break
        brace = s.index('{', m.start())
        head = s[pos:brace]
        end = scan_block(s, brace)
        body = s[brace+1:end]
        fm = re.search(r'file:\s*([^,\n]+),', body)
        lm = re.search(r'\bline:\s*([^,\n]+),', body)
        cm = re.search(r'\bcol:\s*([^,\n]+),', body)
        mm = re.search(r'\bmessage:\s*(.*)', body, re.DOTALL)
        if not (fm and lm and cm and mm) or f'stage: "{stage}"' not in body:
            out.append(head + s[brace:end+1])
            pos = end + 1
            continue
        filex = fm.group(1).strip()
        linex = lm.group(1).strip()
        colex = cm.group(1).strip()
        msg = mm.group(1).rstrip().rstrip(',').rstrip()
        if filex == self_file_expr:
            repl = f'{self_helper}({linex}, {colex}, {msg})'
            count_self += 1
        else:
            repl = f'Diag::at("{stage}", {filex}, {linex}, {colex}, {msg})'
            count_at += 1
        out.append(head + repl)
        pos = end + 1
    open(path, 'w', encoding='utf-8', newline='\n').write(''.join(out))
    print(f'{path}: {count_self} -> {self_helper}, {count_at} -> Diag::at')

convert('src/typecheck.rs', 'type', 'self.cur_file', 'self.err')
convert('src/parser.rs', 'parse', 'self.toks[self.idx].pos.file', 'self.perr')
