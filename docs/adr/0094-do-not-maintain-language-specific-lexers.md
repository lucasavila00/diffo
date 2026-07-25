# ADR 0094: Do not maintain language-specific lexers

Status: Rejected

Evaluates
[Use lightweight language lexers](../highlight-performance/09-lightweight-lexers.md).

## Problem

A small Rust or JSON lexer could recognize comments, strings, numbers, and keywords
with less work than syntect. Syntect would remain the fallback for other languages.

## Result

The syntax snapshots show that Diffo's colors are not based only on token spelling.
For example, Rust function names and ordinary identifiers have different colors.
JSON property names and string values also need different roles.

A lexer that only splits text into lexical tokens cannot reproduce those results.
Adding enough context to match them means building and maintaining language-specific
parsers, including multiline state and language evolution. Benchmarking an
incomplete lexer would measure different output and would not satisfy the
hypothesis's correctness condition.

## Decision

Do not add custom Rust or JSON lexers. Keep the bat-curated syntect definitions as
the single source of language behavior.

Consider a specialized backend only when an existing maintained parser can provide
equivalent roles without making Diffo own language grammars.
