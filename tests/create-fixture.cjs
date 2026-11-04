const fs = require('node:fs');

function fixturePDF(destination) {
  const objects = [];
  const add = body => { objects.push(body); return objects.length; };
  const catalog = add('');
  const pagesRoot = add('');
  const serif = add('<< /Type /Font /Subtype /Type1 /BaseFont /Times-Roman >>');
  const sans = add('<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>');
  const pages = [];
  const escape = text => text.replace(/[\\()]/g, match => `\\${match}`);
  const headings = ['A quieter way to read', 'Follow your curiosity', 'Keep the useful pieces', 'A room for your thoughts', 'Take your time', 'Return to the page'];
  for (let number = 1; number <= 6; number++) {
    const lines = [
      'A book asks very little of its reader: a little time, a little attention,',
      'and the willingness to pause. Good reading tools should ask even less.',
      'The words belong in the foreground. Everything else can be quiet.',
      '',
      'When an idea catches your curiosity, keep it close. Select a sentence,',
      'copy a passage, or capture the diagram that makes everything clear.',
      'The useful details are often the ones we want to discuss with others.',
      '',
      'There is no single correct pace. Turn a page when you are ready.',
      'Leave a bookmark when you need to go. Your place will be waiting.',
    ];
    const stream = [
      '0.98 0.975 0.96 rg 0 0 595 842 re f',
      '0.22 0.35 0.31 rg 48 741 30 3 re f',
      `BT /F2 9 Tf 0.35 0.40 0.37 rg 48 763 Td (SUMI READING NOTES / ${String(number).padStart(2, '0')}) Tj ET`,
      `BT /F1 30 Tf 0.12 0.16 0.14 rg 48 694 Td (${escape(headings[number - 1])}) Tj ET`,
      ...lines.map((line, index) => `BT /F1 13 Tf 0.23 0.25 0.23 rg 48 ${638 - index * 23} Td (${escape(line)}) Tj ET`),
      '0.89 0.92 0.88 rg 48 218 499 118 re f',
      '0.36 0.48 0.40 rg 68 238 80 78 re f',
      '0.60 0.68 0.56 rg 158 238 80 50 re f',
      '0.76 0.79 0.65 rg 248 238 80 65 re f',
      'BT /F2 10 Tf 0.23 0.32 0.27 rg 350 285 Td (A little space.) Tj 0 -18 Td (A little focus.) Tj 0 -18 Td (A better reading day.) Tj ET',
      `BT /F2 9 Tf 0.45 0.48 0.45 rg 48 53 Td (Sumi - original test document) Tj 475 0 Td (${number}) Tj ET`,
    ].join('\n');
    const content = add(`<< /Length ${Buffer.byteLength(stream)} >>\nstream\n${stream}\nendstream`);
    pages.push(add(`<< /Type /Page /Parent ${pagesRoot} 0 R /MediaBox [0 0 595 842] /Resources << /Font << /F1 ${serif} 0 R /F2 ${sans} 0 R >> >> /Contents ${content} 0 R >>`));
  }
  const outlineRoot = add('');
  const outlineItems = headings.map((heading, index) => add(`<< /Title (${escape(heading)}) /Parent ${outlineRoot} 0 R /Dest [${pages[index]} 0 R /Fit] >>`));
  outlineItems.forEach((id, index) => { objects[id - 1] = objects[id - 1].replace(' >>', `${index ? ` /Prev ${outlineItems[index - 1]} 0 R` : ''}${index < outlineItems.length - 1 ? ` /Next ${outlineItems[index + 1]} 0 R` : ''} >>`); });
  objects[outlineRoot - 1] = `<< /Type /Outlines /First ${outlineItems[0]} 0 R /Last ${outlineItems.at(-1)} 0 R /Count ${outlineItems.length} >>`;
  objects[catalog - 1] = `<< /Type /Catalog /Pages ${pagesRoot} 0 R /Outlines ${outlineRoot} 0 R >>`;
  objects[pagesRoot - 1] = `<< /Type /Pages /Kids [${pages.map(id => `${id} 0 R`).join(' ')}] /Count ${pages.length} >>`;
  let pdf = '%PDF-1.7\n';
  const offsets = [0];
  objects.forEach((body, index) => { offsets.push(Buffer.byteLength(pdf)); pdf += `${index + 1} 0 obj\n${body}\nendobj\n`; });
  const xref = Buffer.byteLength(pdf);
  pdf += `xref\n0 ${offsets.length}\n0000000000 65535 f \n${offsets.slice(1).map(offset => `${String(offset).padStart(10, '0')} 00000 n \n`).join('')}trailer\n<< /Size ${offsets.length} /Root ${catalog} 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  fs.writeFileSync(destination, pdf);
  return destination;
}

module.exports = { fixturePDF };
