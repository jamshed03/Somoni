# Templates

Files `name.template.smn` are HTML with values and directives. This site is built that way, one file per page.

```template
@props(title: string, pages: list<Page>)
<h1>{{ title }}</h1>
<ul>
@foreach (page: Page in pages)
    <li>{{ page.title }}</li>
@endforeach
</ul>
```

- `{{ }}` always escapes, `{!! !!}` writes raw HTML
- `@if`, `@elseif`, `@else`, `@foreach`, `@include`
- `@include(card(page))` alone in an indented line indents every line the other template writes, so the HTML stays readable (`<pre>` in that template gets the spaces too)
- `capture(fn() => page(...))` gives a page as a string, `write_file` saves it
- A typo in a template is an error before the program runs, with file and line
