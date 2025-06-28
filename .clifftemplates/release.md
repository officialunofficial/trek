# Release {{ version }}

## What's Changed

{% for group, commits in commits | group_by(attribute="group") %}
### {{ group | upper_first }}
{% for commit in commits %}
- {% if commit.breaking %}[**BREAKING**] {% endif %}{% if commit.scope %}**{{ commit.scope }}:** {% endif %}{{ commit.message | upper_first }} by @{{ commit.author.name }} in {{ commit.id | truncate(length=7, end="") }}
{%- endfor %}
{% endfor %}

## Full Changelog

https://github.com/officialunofficial/trek/compare/{{ previous.version }}...{{ version }}