const PACKAGE_MACROS: &str = r#"
      {% macro package_row(record) %}
      <tr>
        <td>{{ record.package }}</td>
        <td>{{ record.version }}</td>
        <td>{{ record.codename }}</td>
        <td>{{ record.architectures }}</td>
      </tr>
      {% endmacro package_row %}
    "#;
const PACKAGE_TABLE: &str = r#"
      <table>
        <thead>
          <th>Package</th>
          <th>Version</th>
          <th></th>
          <th>Architecture</th>
        </thead>
      {% for _, package_records in madison %}
        {% for record in package_records %}
          {{ package_macros::package_row(record=record) }}
        {% endfor %}
      {% endfor %}
      </table>
    "#;
const SEARCH_FORM: &str = r#"
      <form method="get">
        <input id="urlInput" type="search" name="package" placeholder="package name" autofocus required>
        <input type="submit">
      </form>
    "#;

const BASE_TMPL: &str = r#"
        <html>
          {% block body %}{% endblock %}
          {% include "footer" ignore missing %}
        </html>
    "#;
const INDEX_TMPL: &str = r#"
        {% extends "base.html" %}
        {% block body %}{% include "search-form" %}{% endblock %}
    "#;
const PACKAGE_TMPL: &str = r#"
        {% extends "base.html" %}
        {% import "package-macros" as package_macros %}
        {% block body %}{% include "package-table" %}{% endblock %}
    "#;
pub(super) const TEMPLATES: &[(&str, &str)] = &[
    ("package-macros", PACKAGE_MACROS),
    ("package-table", PACKAGE_TABLE),
    ("search-form", SEARCH_FORM),
    ("base.html", BASE_TMPL),
    ("index.html", INDEX_TMPL),
    ("package.html", PACKAGE_TMPL),
];
