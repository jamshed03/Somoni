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
- `capture(fn() => page(...))` gives a page as a string, `write_file` saves it
- A typo in a template is an error before the program runs, with file and line
