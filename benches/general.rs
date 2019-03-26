use criterion::{criterion_group, criterion_main, Criterion};
use unicode_linebreak::Strict;

criterion_main!(benches);
criterion_group!(benches, general, reuse, reuse_no_splitting);

const TEXT: &str = "
Examples: $(12.35) 2,1234 (12)¢ 12.54¢
᚛ᚈᚑᚋ ᚄᚉᚑᚈᚈ᚜ and ᚛ᚑᚌᚐᚋ᚜﻿okok
Examples: ‘9...’, ‘a...’, ‘H...’
[435] 🇦🇨🇩🇪🇪 Do not break within ‘——’, even with intervening spaces. bar ” [ foo
Do not break after [ even after spaces.
Example pairs: ‘$9’, ‘$[’, ‘$-’, ‘-9’, ‘/9’, ‘99’, ‘,9’, ‘9%’ ‘]%’
Some Emoji 👨‍👧‍👦 Okok
日本国（にほんこく、にっぽんこく）、または日本（にほん、にっぽん）は、日本列島（北海道・本州・四国・九州の主要四島およびそれに付随する島々）及び、南西諸島・伊豆諸島・小笠原諸島などから成る東アジアの島国[1][2]。議会制民主主義国家である。首都は東京都。

気候は四季の変化に富み、その国土の多くは山地で、人口は平野部に集中している。国内には47の都道府県があり、日本人や少数の先住民族のアイヌおよび外国人系の人々が居住し、事実上の公用語として日本語が使用される。内政では、明治維新を経て立憲国家となり、第二次世界大戦後の1947年にGHQの指導の下、現行の日本国憲法を施行。1940年代に起きた太平洋戦争からの復興を遂げ、1960年代からの高度経済成長により工業化が加速し、科学技術立国が推進された結果経済大国にもなったが、1980年代末のバブル崩壊後はITの研究開発で後れを取り、他の先進国と比較して全面的に後退しつつある。
また先進国のひとつとして数えられており、G7、G8およびG20のひとつ。外交では、1956年から国際連合に加盟し、国連中心主義をとっている[3]。
The purpose of this rule is to prevent breaks in common cases where a part of a word appears between
delimiters—for
example, in “person(s)”.
The expression [^SP, BA, HY] designates any line break class other than SP, BA or HY. The symbol ^ is used, instead
of !, to avoid confusion with the use of ! to indicate an explicit break. Unlike the case for WJ, inserting a SP
overrides the non-breaking nature of a GL. Allowing a break after BA or HY matches widespread implementation
practice and supports a common way of handling special line breaking of explicit hyphens, such as in Polish and
Portuguese. See Section 5.3, Use of Hyphen.
";

fn general(c: &mut Criterion) {
    c.bench_function("General", move |b| {
        b.iter(|| unicode_linebreak::break_lines(TEXT, Strict).for_each(drop))
    });
}

fn reuse(c: &mut Criterion) {
    let mut vec = unicode_linebreak::AnnotatedVec::new();
    c.bench_function("Reuse", move |b| {
        b.iter(|| {
            vec.clear();
            vec.extend_str(TEXT, Strict);
            vec.split_str_iter(TEXT).for_each(drop);
        })
    });
}

fn reuse_no_splitting(c: &mut Criterion) {
    let mut vec = unicode_linebreak::AnnotatedVec::new();
    c.bench_function("Reuse (no splitting)", move |b| {
        b.iter(|| {
            vec.clear();
            vec.extend_str(TEXT, Strict);
        })
    });
}
