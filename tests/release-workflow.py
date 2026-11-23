#!/usr/bin/env python3
"""Build and upload refs must agree even when dispatch runs from main."""
from pathlib import Path
import re

workflow = (Path(__file__).resolve().parents[1] / '.github/workflows/release.yml').read_text()
checkout = workflow.split('actions/checkout@', 1)[1].split('- name:', 1)[0]
assert re.search(r'ref:\s*refs/tags/\$\{\{ inputs.tag_name \}\}', checkout), 'release checkout ignores the requested tag'
assert 'git rev-parse --verify "refs/tags/${RELEASE_TAG}^{commit}"' in workflow
assert 'git rev-parse HEAD' in workflow
assert 'tag_name: ${{ inputs.tag_name }}' in workflow
print('release builds verify and upload the requested tag')
